# The problems of a baseline file of FAST_FEEDBACK, one line each; nothing when it is valid.
def known_top: ["$comment", "margin", "samples", "cap_seconds", "max_age_days", "max_runs", "scenarios"];
def known_scenario: [
  "id", "describe", "baseline_seconds", "cap_seconds", "margin", "over_cap", "todo",
  "command", "warmup",
  "workflow", "job", "job_name", "step", "event", "branch", "attempt",
  "skipped_steps", "ran_steps", "max_changed_files"
];
def positive: type == "number" and . > 0;
def fraction: type == "number" and . > 0 and . < 1;
def strings: type == "array" and all(.[]; type == "string");

. as $top
| if type != "object" then "the file must be a JSON object" else
  ( (keys - known_top)[] | "unknown field `\(.)`" ),
  ( if (.margin | fraction) then empty else "`margin` must be a fraction between 0 and 1, chosen from measured noise (README of FAST_FEEDBACK)" end ),
  ( if (.samples | type == "number" and . >= 1 and . == floor) then empty else "`samples` must be a whole number of at least 1: the median of how many samples each check takes" end ),
  ( if has("cap_seconds") and (.cap_seconds | positive | not) then "`cap_seconds` must be a positive number" else empty end ),
  ( if has("max_age_days") and (.max_age_days | positive | not) then "`max_age_days` must be a positive number" else empty end ),
  ( if has("max_runs") and (.max_runs | positive | not) then "`max_runs` must be a positive number" else empty end ),
  ( if (.scenarios | type == "array" and length > 0) | not then "`scenarios` must be a non-empty list" else
      ( [.scenarios[] | .id] | group_by(.)[] | select(length > 1) | "scenario id `\(.[0])` is used \(length) times" ),
      ( .scenarios | to_entries[] | .key as $i | .value as $s
        | ($s.id // "#\($i)") as $id
        | ( $s.cap_seconds // $top.cap_seconds // 180 ) as $cap
        | if ($s | type) != "object" then "scenario #\($i) must be an object" else
          ( ($s | keys) - known_scenario | .[] | "scenario `\($id)`: unknown field `\(.)`" ),
          ( if ($s.id | type == "string" and test("^[a-z0-9][a-z0-9-]*$")) then empty else "scenario #\($i): `id` must be lowercase words joined by dashes" end ),
          ( if ($s.baseline_seconds | positive) then empty
            elif ($s | has("baseline_seconds") | not) and ($s.todo | type == "string" and length > 0) then empty
            else "scenario `\($id)`: `baseline_seconds` must be a positive number, or left out with a `todo` saying when the scenario gets one" end ),
          ( if ($s | has("command")) == ($s | has("workflow")) then "scenario `\($id)`: give either `command` (run here) or `workflow` and `job` (measured from the GitHub Actions history)" else empty end ),
          ( if ($s | has("workflow")) and (($s.job | type) != "string") then "scenario `\($id)`: `workflow` needs the id of the `job` it measures" else empty end ),
          ( if ($s | has("command")) and ($s.command | type) != "string" then "scenario `\($id)`: `command` must be a string" else empty end ),
          ( if ($s | has("attempt")) and ([$s.attempt] | inside(["first", "rerun", "any"]) | not) then "scenario `\($id)`: `attempt` must be first, rerun or any" else empty end ),
          ( if ($s | has("event")) and (($s.event | type) != "string") and (($s.event | strings) | not) then "scenario `\($id)`: `event` must be an event name or a list of them" else empty end ),
          ( [ "skipped_steps", "ran_steps" ][] as $k | if ($s | has($k)) and ($s[$k] | strings | not) then "scenario `\($id)`: `\($k)` must be a list of step names" else empty end ),
          ( if ($s | has("margin")) and ($s.margin | fraction | not) then "scenario `\($id)`: `margin` must be a fraction between 0 and 1" else empty end ),
          ( if ($s | has("cap_seconds")) and ($s.cap_seconds | positive | not) then "scenario `\($id)`: `cap_seconds` must be a positive number" else empty end ),
          ( if ($s.baseline_seconds | positive) and $s.baseline_seconds > $cap and ($s.over_cap != true)
            then "scenario `\($id)`: baseline \($s.baseline_seconds) s is over the cap of \($cap) s; make it faster, or mark it `over_cap` with a `todo` naming the work that brings it under"
            else empty end ),
          ( if $s.over_cap == true and ($s | has("baseline_seconds") | not) then "scenario `\($id)`: `over_cap` needs a baseline" else empty end ),
          ( if $s.over_cap == true and (($s.todo | type == "string" and length > 0) | not) then "scenario `\($id)`: `over_cap` needs a `todo` naming the work that brings it under the cap" else empty end ),
          ( if $s.over_cap == true and ($s.baseline_seconds | positive) and $s.baseline_seconds <= $cap then "scenario `\($id)`: baseline \($s.baseline_seconds) s is under the cap of \($cap) s; remove `over_cap` and `todo`" else empty end ),
          ( if ($s | has("over_cap")) and ($s.over_cap | type) != "boolean" then "scenario `\($id)`: `over_cap` must be true or false" else empty end )
          end )
    end )
  end
