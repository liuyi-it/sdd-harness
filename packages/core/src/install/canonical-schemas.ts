export const CANONICAL_SCHEMAS = {
  "config.schema.json": `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/config.schema.json",
  "title": "sdd-harness configuration",
  "type": "object",
  "required": [
    "schemaVersion",
    "project",
    "plugins",
    "codebase",
    "workflow",
    "quality",
    "security"
  ],
  "properties": {
    "schemaVersion": { "const": "1.2.0" },
    "project": {
      "type": "object",
      "required": ["name"],
      "properties": { "name": { "type": "string", "minLength": 1 } },
      "additionalProperties": true
    },
    "plugins": { "type": "object" },
    "codebase": { "type": "object" },
    "workflow": { "type": "object" },
    "quality": { "type": "object" },
    "security": { "type": "object" }
  },
  "additionalProperties": true
}
`,
  "state.schema.json": `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/state.schema.json",
  "title": "sdd-harness workflow state",
  "type": "object",
  "required": [
    "schemaVersion",
    "version",
    "updatedAt",
    "initialized",
    "currentPhase",
    "indexStatus",
    "tasks",
    "artifacts"
  ],
  "properties": {
    "schemaVersion": { "const": "1.2.0" },
    "version": { "type": "integer", "minimum": 1 },
    "updatedAt": { "type": "string", "format": "date-time" },
    "initialized": { "type": "boolean" },
    "activeLoop": {},
    "currentPhase": {
      "enum": [
        "NOT_INITIALIZED",
        "INITIALIZING",
        "INDEXING",
        "INDEX_READY",
        "NEW_STARTED",
        "CLARIFYING",
        "SPEC_READY",
        "DESIGNING",
        "DESIGN_READY",
        "PLANNING",
        "PLAN_READY",
        "BUILDING",
        "BUILD_READY",
        "VERIFYING",
        "VERIFY_READY",
        "REVIEWING",
        "REVIEW_READY",
        "ARCHIVING",
        "ARCHIVED",
        "FAILED",
        "PAUSED"
      ]
    },
    "indexStatus": {
      "enum": ["MISSING", "INDEXING", "INDEX_READY", "STALE", "UNAVAILABLE"]
    },
    "tasks": {
      "type": "object",
      "additionalProperties": {
        "enum": ["PENDING", "BUILDING", "DONE", "FAILED", "SKIPPED"]
      }
    },
    "artifacts": {
      "type": "object",
      "additionalProperties": {
        "enum": ["MISSING", "READY", "CANDIDATE", "STALE"]
      }
    }
  },
  "additionalProperties": true
}
`,
  "task.schema.json": `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/task.schema.json",
  "title": "sdd-harness task",
  "type": "object",
  "required": [
    "id",
    "title",
    "phase",
    "status",
    "requirements",
    "scenarios",
    "dependsOn",
    "allowedFiles",
    "verification",
    "doneCriteria"
  ],
  "properties": {
    "id": {
      "type": "string",
      "pattern": "^TASK-[0-9]{3}(?:-(?:RED|GREEN|REFACTOR|VERIFY))?$"
    },
    "title": { "type": "string", "minLength": 1 },
    "phase": { "enum": ["RED", "GREEN", "REFACTOR", "VERIFY"] },
    "status": { "enum": ["PENDING", "BUILDING", "DONE", "FAILED", "SKIPPED"] },
    "requirements": {
      "type": "array",
      "items": { "type": "string", "pattern": "^REQ-[0-9]{3,}$" },
      "minItems": 1
    },
    "scenarios": {
      "type": "array",
      "items": { "type": "string", "pattern": "^REQ-[0-9]{3,}-SC-[0-9]{3,}$" },
      "minItems": 1
    },
    "dependsOn": { "type": "array", "items": { "type": "string" } },
    "allowedFiles": {
      "type": "array",
      "items": { "type": "string" },
      "minItems": 1
    },
    "expectedNewFiles": { "type": "array", "items": { "type": "string" } },
    "forbiddenFiles": { "type": "array", "items": { "type": "string" } },
    "verification": {
      "type": "array",
      "items": { "type": "string" },
      "minItems": 1
    },
    "doneCriteria": {
      "type": "array",
      "items": { "type": "string" },
      "minItems": 1
    }
  },
  "additionalProperties": false
}
`,
  "task-execution-result.schema.json": `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/task-execution-result.schema.json",
  "title": "sdd-harness task execution result",
  "type": "object",
  "required": [
    "schemaVersion",
    "taskId",
    "status",
    "summary",
    "commandEvidence",
    "fileDelta",
    "timestamps"
  ],
  "properties": {
    "schemaVersion": { "const": "1.2.0" },
    "taskId": {
      "type": "string",
      "pattern": "^TASK-[0-9]{3}(?:-(?:RED|GREEN|REFACTOR|VERIFY))?$"
    },
    "status": {
      "enum": ["SUCCEEDED", "FAILED", "BLOCKED", "SKIPPED", "DEGRADED"]
    },
    "summary": { "type": "string", "minLength": 1 },
    "commandEvidence": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "object",
        "required": ["command", "args", "outputSummary"],
        "properties": {
          "command": { "type": "string", "minLength": 1 },
          "args": {
            "type": "array",
            "items": { "type": "string" }
          },
          "exitCode": { "type": "integer", "minimum": 0 },
          "outputSummary": { "type": "string", "minLength": 1 }
        },
        "additionalProperties": false
      }
    },
    "fileDelta": {
      "type": "object",
      "required": ["added", "modified", "deleted"],
      "properties": {
        "added": { "type": "array", "items": { "type": "string" } },
        "modified": { "type": "array", "items": { "type": "string" } },
        "deleted": { "type": "array", "items": { "type": "string" } }
      },
      "additionalProperties": false
    },
    "timestamps": {
      "type": "object",
      "required": ["startedAt", "endedAt"],
      "properties": {
        "startedAt": { "type": "string", "format": "date-time" },
        "endedAt": { "type": "string", "format": "date-time" }
      },
      "additionalProperties": false
    },
    "mode": {
      "type": "object",
      "required": ["requested", "actual"],
      "properties": {
        "requested": { "enum": ["subagent", "main-agent"] },
        "actual": { "enum": ["subagent", "main-agent"] }
      },
      "additionalProperties": false
    },
    "notes": {
      "type": "array",
      "items": { "type": "string", "minLength": 1 }
    },
    "legacy": {
      "type": "object"
    }
  },
  "additionalProperties": false
}
`,
  "loop.schema.json": `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/loop.schema.json",
  "title": "sdd-harness loop specification",
  "type": "object",
  "required": [
    "schemaVersion",
    "loopId",
    "mode",
    "maxSteps",
    "stoppingRules",
    "createdAt",
    "updatedAt"
  ],
  "properties": {
    "schemaVersion": { "const": "1.2.0" },
    "loopId": { "type": "string", "minLength": 1 },
    "mode": { "enum": ["auto"] },
    "maxSteps": { "type": "integer", "minimum": 1 },
    "stoppingRules": {
      "type": "array",
      "minItems": 1,
      "items": { "type": "string", "minLength": 1 }
    },
    "createdAt": { "type": "string", "format": "date-time" },
    "updatedAt": { "type": "string", "format": "date-time" }
  },
  "additionalProperties": false
}
`,
  "loop-run.schema.json": `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/loop-run.schema.json",
  "title": "sdd-harness loop run",
  "type": "object",
  "required": [
    "schemaVersion",
    "runId",
    "loopId",
    "status",
    "startedAt",
    "steps"
  ],
  "properties": {
    "schemaVersion": { "const": "1.2.0" },
    "runId": { "type": "string", "minLength": 1 },
    "loopId": { "type": "string", "minLength": 1 },
    "status": {
      "enum": [
        "PENDING",
        "RUNNING",
        "PAUSED",
        "SUCCEEDED",
        "FAILED",
        "ABORTED",
        "ARCHIVED"
      ]
    },
    "startedAt": { "type": "string", "format": "date-time" },
    "endedAt": { "type": "string", "format": "date-time" },
    "steps": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["step", "command", "status", "startedAt", "endedAt"],
        "properties": {
          "step": { "type": "integer", "minimum": 1 },
          "command": { "type": "string", "minLength": 1 },
          "status": {
            "enum": ["SUCCEEDED", "FAILED", "BLOCKED", "SKIPPED", "PAUSED"]
          },
          "startedAt": { "type": "string", "format": "date-time" },
          "endedAt": { "type": "string", "format": "date-time" }
        },
        "additionalProperties": false
      }
    }
  },
  "additionalProperties": false
}
`,
  "artifact-metadata.schema.json": `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/artifact-metadata.schema.json",
  "title": "sdd-harness artifact metadata",
  "type": "object",
  "required": [
    "schemaVersion",
    "generatedBy",
    "inputHash",
    "artifactHash",
    "createdAt"
  ],
  "properties": {
    "schemaVersion": { "const": "1.0.0" },
    "generatedBy": { "const": "sdd-harness" },
    "inputHash": { "type": "string", "pattern": "^sha256:[a-f0-9]{64}$" },
    "artifactHash": { "type": "string", "pattern": "^sha256:[a-f0-9]{64}$" },
    "createdAt": { "type": "string", "format": "date-time" }
  },
  "additionalProperties": false
}
`,
} as const;
