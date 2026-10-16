#!/usr/bin/env python3
"""Execute explicit scenario bindings and normalize their evidence to SARIF."""

import argparse
from collections import Counter
from dataclasses import dataclass
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
import time
from urllib.parse import unquote, urlparse
import xml.etree.ElementTree as ET


OBLIGATIONS = {
    "REPLAY_SAFE": ("same-operation", "distinct-operations", "retry-after-failure"),
    "CONCURRENT_TRANSITIONS": ("competing-transitions", "invariant-after-contention"),
    "DURABLE_WORK": ("accepted-restart", "retryable-failure", "terminal-outcome"),
    "RECOVERABLE_EFFECTS": ("partial-failure", "recovery", "repeated-recovery"),
    "SCOPED_WORK": ("owner-close", "late-completion", "failure-cleanup"),
    "DATA_ISOLATION": ("cross-identity-read", "cross-identity-write", "identity-switch"),
}
MAX_BYTES = 8 * 1024 * 1024
BINDING_FIELDS = {"command", "scenarios", "runner", "cwd", "timeout_seconds"}


class EvidenceError(Exception):
    """Evidence was unavailable or cannot be trusted."""


@dataclass
class Test:
    name: str
    status: str
    file: str | None = None


def require(condition, message):
    if not condition:
        raise EvidenceError(message)


def nonempty(value):
    return isinstance(value, str) and bool(value.strip()) and "\0" not in value


def load_json(text):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f"Duplicate JSON field: {key}")
            result[key] = value
        return result

    try:
        return json.loads(text, object_pairs_hook=unique,
                          parse_constant=lambda value: require(False, f"Invalid JSON number: {value}"))
    except (ValueError, UnicodeError) as error:
        raise EvidenceError(f"Invalid JSON: {error}") from error


def read_bounded(path):
    require(path.is_file() and not path.is_symlink(), f"Missing or nonregular report: {path}")
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    require(len(data) <= MAX_BYTES, "Report exceeds the 8 MiB limit")
    try:
        return data.decode("utf-8")
    except UnicodeError as error:
        raise EvidenceError("Report is not UTF-8") from error


def inside(root, value, label):
    require(nonempty(value) and not Path(value).is_absolute(), f"{label} must be a relative path")
    path = (root / value).resolve()
    require(path.is_relative_to(root), f"{label} escapes the checked project")
    return path


def load_project(identity, root):
    supplied = load_json(os.environ.get("REQFILE_BINDING", "{}"))
    require(isinstance(supplied, dict), "REQFILE_BINDING must be an object")
    require(not (supplied.keys() - BINDING_FIELDS - {"binding", "extra_scenarios"}), "Unknown adapter binding input")
    name = supplied.get("binding", f".reqfile/{identity}/binding.json")
    path = inside(root, name, "binding")
    direct = {key: value for key, value in supplied.items() if key in BINDING_FIELDS}
    if "command" in direct and "scenarios" in direct and "binding" not in supplied:
        binding = {}
    else:
        require(path.exists(), f"Missing project binding: {name}; declare command and scenarios")
        binding = load_json(read_bounded(path))
        require(isinstance(binding, dict), "Project binding must be an object")
        require(not (binding.keys() - BINDING_FIELDS), "Unknown project binding field")
    if "runner" in binding and "runner" in direct:
        require(binding["runner"] == direct["runner"], "Binding runner conflicts with the selected language adapter")
    binding.update(direct)
    return binding, supplied, name


def binding_for(identity, root):
    binding, supplied, name = load_project(identity, root)
    command = binding.get("command")
    require(isinstance(command, list) and command and all(nonempty(x) for x in command),
            "command must be a nonempty argv array of nonempty strings")
    require(sum(len(x) for x in command) <= 32768, "command exceeds the 32 KiB limit")
    scenarios = binding.get("scenarios")
    require(isinstance(scenarios, dict) and all(nonempty(x) for x in scenarios.values()),
            "scenarios must map obligations to exact nonempty test names")
    extras = supplied.get("extra_scenarios", [])
    require(isinstance(extras, list) and all(nonempty(value) for value in extras), "extra_scenarios must be a list of names")
    require(len(set(extras)) == len(extras) and not (set(extras) & set(OBLIGATIONS[identity])), "Duplicate scenario obligation")
    required = set(OBLIGATIONS[identity]) | set(extras)
    require(set(scenarios) == required,
            f"Scenario obligations must be exactly {', '.join(sorted(required))}")
    require(len(set(scenarios.values())) == len(scenarios), "Each obligation must name a different test")
    runner = binding.get("runner", "junit")
    require(runner in ("junit", "vitest", "dart"), "runner must be junit, vitest, or dart")
    cwd = inside(root, binding.get("cwd", "."), "cwd")
    require(cwd.is_dir(), "cwd is not a directory")
    timeout = binding.get("timeout_seconds", 60)
    require(type(timeout) is int and 1 <= timeout <= 300, "timeout_seconds must be an integer from 1 through 300")
    return binding, command, scenarios, runner, cwd, timeout, name


def terminate(process):
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    else:
        process.kill()
    process.wait()


def execute(command, cwd, env, timeout):
    try:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=True)
    except OSError as error:
        raise EvidenceError(f"Cannot start {command[0]}: {error}") from error
    output = {"stdout": bytearray(), "stderr": bytearray()}
    deadline = time.monotonic() + timeout
    try:
        with selectors.DefaultSelector() as selector:
            for name, stream in (("stdout", process.stdout), ("stderr", process.stderr)):
                selector.register(stream, selectors.EVENT_READ, name)
            while selector.get_map():
                remaining = deadline - time.monotonic()
                require(remaining > 0, f"Command timed out after {timeout} seconds")
                for key, _ in selector.select(min(remaining, 0.1)):
                    chunk = os.read(key.fd, 65536)
                    if chunk:
                        output[key.data].extend(chunk)
                        require(sum(map(len, output.values())) <= MAX_BYTES, "Command output exceeds the 8 MiB limit")
                    else:
                        selector.unregister(key.fileobj)
            remaining = deadline - time.monotonic()
            require(remaining > 0, f"Command timed out after {timeout} seconds")
            try:
                code = process.wait(timeout=remaining)
            except subprocess.TimeoutExpired as error:
                raise EvidenceError(f"Command timed out after {timeout} seconds") from error
    finally:
        # Kill surviving descendants too: they must not keep mutating fixtures after a check.
        terminate(process)
        process.stdout.close()
        process.stderr.close()
    try:
        return code, output["stdout"].decode("utf-8"), output["stderr"].decode("utf-8")
    except UnicodeError as error:
        raise EvidenceError("Command output is not UTF-8") from error


def junit(text):
    require("<!DOCTYPE" not in text.upper() and "<!ENTITY" not in text.upper(), "JUnit entities and DTDs are not supported")
    try:
        root = ET.fromstring(text)
    except ET.ParseError as error:
        raise EvidenceError(f"Malformed JUnit: {error}") from error
    require(root.tag in ("testsuite", "testsuites"), "JUnit root must be testsuite or testsuites")
    cases = list(root.iter("testcase"))
    for suite in [root, *root.iter("testsuite")]:
        require(not any(child.tag in ("error", "failure", "skipped") for child in suite),
                "JUnit suite failed or skipped outside a reported testcase")
        descendants = list(suite.iter("testcase"))
        counts = {
            "tests": len(descendants),
            "errors": sum(case.find("error") is not None for case in descendants),
            "failures": sum(case.find("failure") is not None for case in descendants),
            "skipped": sum(case.find("skipped") is not None or case.get("status") in
                           ("notrun", "skipped", "disabled") for case in descendants),
        }
        for field, expected in counts.items():
            if field in suite.attrib:
                count = suite.attrib[field]
                require(count.isdecimal() and int(count) == expected, f"JUnit {field} count is inconsistent")
        if "disabled" in suite.attrib:
            require(suite.attrib["disabled"] == "0", "JUnit suite contains disabled tests")
    tests = []
    for case in cases:
        name = case.get("name")
        require(nonempty(name), "JUnit testcase has no name")
        status = case.get("status", "run")
        require(status in ("run", "passed", "notrun", "skipped", "disabled"), f"Unknown JUnit status: {status}")
        kinds = {child.tag for child in case}
        require(not (kinds - {"failure", "error", "skipped", "system-out", "system-err", "properties"}), "Unknown JUnit testcase outcome")
        verdict = "failed" if kinds & {"failure", "error"} else (
            "skipped" if "skipped" in kinds or status in ("notrun", "skipped", "disabled") else "passed")
        tests.append(Test(name, verdict, case.get("file")))
    return tests


def vitest(text):
    report = load_json(text)
    require(isinstance(report, dict) and type(report.get("success")) is bool, "Vitest report has no final success status")
    suites = report.get("testResults")
    require(isinstance(suites, list), "Vitest testResults must be a list")
    tests = []
    for suite in suites:
        require(isinstance(suite, dict) and suite.get("status") in ("passed", "failed"), "Vitest suite is incomplete")
        assertions = suite.get("assertionResults")
        require(isinstance(assertions, list) and assertions, "Vitest suite has no test results")
        for result in assertions:
            require(isinstance(result, dict) and nonempty(result.get("fullName")), "Vitest assertion has no fullName")
            status = result.get("status")
            require(status in ("passed", "failed", "pending", "skipped", "todo", "disabled"), "Unknown Vitest test outcome")
            tests.append(Test(result["fullName"], status if status in ("passed", "failed") else "skipped", suite.get("name")))
        require(suite["status"] != "failed" or any(x.get("status") == "failed" for x in assertions), "Vitest suite failed outside a reported test")
    for field, count in (("numTotalTests", len(tests)), ("numFailedTests", sum(t.status == "failed" for t in tests))):
        if field in report:
            require(type(report[field]) is int and report[field] == count, f"Vitest {field} is inconsistent")
    require(report["success"] or any(t.status == "failed" for t in tests), "Vitest failed outside a reported test")
    require(not report["success"] or not any(t.status == "failed" for t in tests), "Vitest success contradicts failed tests")
    return tests


def dart(text):
    started, finished, suites = {}, set(), {}
    tests = []
    complete = None
    for line in text.splitlines():
        event = load_json(line)
        require(isinstance(event, dict) and nonempty(event.get("type")), "Invalid Dart event")
        require(complete is None, "Dart events occur after completion")
        kind = event["type"]
        if kind == "suite":
            suite = event.get("suite")
            require(isinstance(suite, dict) and type(suite.get("id")) is int, "Invalid Dart suite")
            suites[suite["id"]] = suite.get("path")
        elif kind == "testStart":
            test = event.get("test")
            require(isinstance(test, dict) and type(test.get("id")) is int and nonempty(test.get("name")), "Invalid Dart test start")
            require(test["id"] not in started, "Duplicate Dart test start")
            started[test["id"]] = test
        elif kind == "testDone":
            identity = event.get("testID")
            require(type(identity) is int and identity in started and identity not in finished, "Unmatched Dart test completion")
            finished.add(identity)
            result = event.get("result")
            require(result in ("success", "failure", "error") and type(event.get("skipped")) is bool
                    and type(event.get("hidden")) is bool, "Invalid Dart test outcome")
            # Hidden load/teardown tests are not scenario evidence, but their failure still blocks.
            if not event["hidden"] or result != "success":
                test = started[identity]
                tests.append(Test(test["name"], "failed" if result != "success" else
                                  ("skipped" if event["skipped"] else "passed"), suites.get(test.get("suiteID"))))
        elif kind == "done":
            require(type(event.get("success")) is bool, "Dart done event lacks success")
            complete = event["success"]
        else:
            require(kind in ("start", "group", "allSuites", "print", "error", "debug", "pause"), f"Unknown Dart event: {kind}")
    require(complete is not None and started.keys() == finished, "Dart report is incomplete")
    require(complete or any(t.status == "failed" for t in tests), "Dart failed outside a reported test")
    require(not complete or not any(t.status == "failed" for t in tests), "Dart success contradicts failed tests")
    return tests


def location(root, cwd, candidate, fallback):
    if nonempty(candidate):
        if candidate.startswith("file://"):
            candidate = unquote(urlparse(candidate).path)
        path = Path(candidate)
        path = (path if path.is_absolute() else cwd / path).resolve()
        if path.is_relative_to(root):
            return path.relative_to(root).as_posix()
    return fallback


def assess(identity, root):
    _, command, scenarios, runner, cwd, timeout, binding = binding_for(identity, root)
    env = os.environ.copy()
    fallback = binding
    for argument in command[1:]:
        if Path(argument).suffix in (".py", ".ts", ".js", ".dart"):
            fallback = location(root, cwd, argument, fallback)
            break
    with tempfile.TemporaryDirectory(prefix="reqfile-scenarios-") as directory:
        report = Path(directory) / "report"
        env["REQFILE_SCENARIO_REPORT"] = str(report)
        argv = list(command)
        if runner == "vitest":
            argv += ["run", "--reporter=json", f"--outputFile={report}"]
        elif runner == "dart":
            argv += ["test", "--reporter=json"]
        code, stdout, _ = execute(argv, cwd, env, timeout)
        require(code >= 0, "Test runner was terminated by a signal")
        if runner == "dart":
            tests = dart(stdout)
        else:
            require(report.exists(), "Test runner produced no fresh report; check the command and runner options")
            text = read_bounded(report)
            tests = junit(text) if runner == "junit" else vitest(text)
    require(tests, "Report contains no executed tests")
    counts = Counter(t.name for t in tests)
    for obligation, name in scenarios.items():
        require(counts[name] == 1, f"{obligation}: expected exactly one test named {name!r}, found {counts[name]}")
    require(not any(t.status == "skipped" for t in tests), "Report contains skipped or pending tests")
    failures = [t for t in tests if t.status == "failed"]
    require(code == 0 or failures, f"Runner exited {code} without a reported failed test")
    return [{"ruleId": identity, "level": "error", "message": {"text": f"Scenario test failed: {test.name}"},
             "locations": [{"physicalLocation": {"artifactLocation": {
                 "uri": location(root, cwd, test.file, fallback)}, "region": {"startLine": 1}}}]} for test in failures]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--requirement", required=True, choices=OBLIGATIONS)
    arguments = parser.parse_args()
    try:
        results = assess(arguments.requirement, Path.cwd().resolve())
    except (EvidenceError, OSError, ValueError) as error:
        print(f"{arguments.requirement}: {error}", file=sys.stderr)
        return 2
    print(json.dumps({"version": "2.1.0", "runs": [{"tool": {"driver": {
        "name": "reqfile-scenarios"}}, "results": results}]}))
    return 1 if results else 0


if __name__ == "__main__":
    sys.exit(main())
