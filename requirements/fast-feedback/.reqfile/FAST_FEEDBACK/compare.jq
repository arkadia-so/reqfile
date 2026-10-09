# The regressions of measured medians against a baseline file, one line each.
# Input: the baseline file; $results[0]: the results (.scenarios.<id>.median).
. as $top
| ($results[0].scenarios) as $measured
| .scenarios[]
| . as $s
| ($measured[$s.id].median // null) as $median
| select($median != null and $s.baseline_seconds != null)  # no baseline yet: measured, never judged
| ($s.margin // $top.margin) as $margin
| ($s.cap_seconds // $top.cap_seconds // 180) as $cap
| ($s.baseline_seconds * (1 + $margin)) as $limit
| def r: . * 10 | round / 10;
  ( if $median > $limit
    then "\($s.id): median \($median | r) s over its baseline \($s.baseline_seconds) s + \($margin * 100 | round)% (\($limit | r) s); find what made it slower, or raise the baseline by hand with the reason in the pull request"
    else empty end ),
  ( if $s.over_cap != true and $median > $cap and $median <= $limit
    then "\($s.id): median \($median | r) s over the cap of \($cap) s; make it faster"
    else empty end )
