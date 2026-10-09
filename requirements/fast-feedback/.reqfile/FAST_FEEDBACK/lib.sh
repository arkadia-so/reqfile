# Shared by the scripts of FAST_FEEDBACK; sourced, not run.

# The workflow files of the repository, one per line.
workflow_files() {
  local file
  for file in .github/workflows/*.yml .github/workflows/*.yaml; do
    [ -f "$file" ] && echo "$file"
  done
  return 0
}

# One line per job of a workflow file: `<on_pr>\t<job id>\t<display name>`, where on_pr is 1
# when the workflow runs on pull_request or pull_request_target, and the display name is the
# job's `name:`, else its id (GitHub then adds the matrix values in parentheses).
# A small reader of block-style YAML, the style workflows are written in: the `on:` value may
# be a word, a flow list or a block; jobs are the keys one level under `jobs:`.
workflow_jobs() {
  awk '
    function indent(s) { match(s, /^ */); return RLENGTH }
    function unquote(s,   c) {
      sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s)
      c = substr(s, 1, 1)
      if (length(s) >= 2 && (c == "\"" || c == q) && substr(s, length(s)) == c) s = substr(s, 2, length(s) - 2)
      return s
    }
    BEGIN { q = "\047" }
    {
      line = $0
      sub(/\r$/, "", line)
      # Drop comments: a # at the start or after a blank (a heuristic: # inside a quoted value
      # after a blank is dropped too, which only matters in a job name).
      sub(/^[ \t]*#.*$/, "", line)
      sub(/[ \t]+#.*$/, "", line)
      if (line ~ /^[ \t]*$/) next
      ind = indent(line)
      if (ind == 0) {
        section = ""
        key = line; sub(/[ \t]*:.*$/, "", key); gsub(/"/, "", key); gsub(q, "", key)
        if ((key == "on" || key == "true") && line ~ /:/) { section = "on"; sub(/^[^:]*:/, "", line); on_text = on_text " " line }
        else if (line ~ /^jobs[ \t]*:/) { section = "jobs"; job_indent = -1 }
        next
      }
      if (section == "on") { on_text = on_text " " line; next }
      if (section != "jobs") next
      if (job_indent < 0) job_indent = ind
      if (ind == job_indent && line ~ /^ *[A-Za-z_][A-Za-z0-9_-]*[ \t]*:/) {
        id = line; sub(/^ */, "", id); sub(/[ \t]*:.*$/, "", id)
        jobs[++count] = id; names[count] = id; prop_indent = -1
        next
      }
      if (ind > job_indent && count > 0) {
        if (prop_indent < 0) prop_indent = ind
        if (ind == prop_indent && line ~ /^ *name[ \t]*:/) {
          value = line; sub(/^ *name[ \t]*:/, "", value)
          value = unquote(value)
          if (value != "") names[count] = value
        }
      }
    }
    END {
      pr = (on_text ~ /(^|[^A-Za-z0-9_])pull_request(_target)?([^A-Za-z0-9_]|$)/) ? 1 : 0
      for (i = 1; i <= count; i++) printf "%d\t%s\t%s\n", pr, jobs[i], names[i]
    }
  ' "$1"
}
