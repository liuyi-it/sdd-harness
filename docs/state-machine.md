# State Machine

The stable path is:

```text
NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
→ BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

Long operations use `INITIALIZING`, `INDEXING`, `NEW_STARTED`, `DESIGNING`, `PLANNING`, `BUILDING`, `VERIFYING`, `REVIEWING`, and `ARCHIVING`. Unanswered blockers use `CLARIFYING`, user cancellation uses `PAUSED`, and execution failures use `FAILED`.

Recovery uses `failedCommand` or `interruptedCommand`. `previousPhase` is the last stable phase and `inProgressPhase` records the interrupted operation. `sdd status` reports the stored `suggestedCommand`.
