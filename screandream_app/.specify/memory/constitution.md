<!--
Sync Impact Report
- Version change: unversioned template -> 1.0.0
- Modified principles: none; initial adoption
- Added sections: Core Principles, Delivery Constraints, Development Workflow, Governance
- Removed sections: none
- Follow-up TODOs: Ratification date requires confirmation from the project owner.
-->
# Screendream Constitution

## Core Principles

### I. User Value First
Every feature MUST address a defined user need and state its expected outcome before implementation
begins. Scope MUST remain limited to the outcome described in its specification; additions require an
explicit scope amendment. This keeps delivery focused on meaningful user value.

### II. Clear, Testable Behavior
Requirements, acceptance criteria, and changes MUST describe observable behavior rather than internal
implementation. Every behavior change MUST be verifiable through automated checks, documented manual
steps, or both. This makes expectations reproducible and reduces ambiguity during review.

### III. Quality at Change Boundaries
Changes MUST preserve existing intended behavior outside their declared scope. New or changed user
flows, data handling, and integrations MUST include proportionate validation covering success, failure,
and boundary cases. This prevents regressions where users are most affected.

### IV. Security and Privacy by Design
Features MUST collect and expose only the data necessary for their stated purpose. Sensitive values
MUST NOT be committed, logged, or displayed to unauthorized users; access and error behavior MUST fail
closed when authorization or validation is uncertain. This protects users and the project from
preventable disclosure.

### V. Simplicity and Maintainability
Solutions MUST use the simplest design that satisfies the approved requirements and reuse established
project patterns when they exist. New dependencies, abstractions, or operational complexity require a
documented need and a clear owner. This keeps the product understandable and sustainable.

## Delivery Constraints

Work MUST have a written specification when it introduces a user-facing capability, materially changes
behavior, or carries security, privacy, or operational risk. Specifications MUST define scope,
assumptions, measurable success criteria, and any dependencies. Out-of-scope behavior MUST be
explicitly identified when its omission could surprise users or stakeholders.

## Development Workflow

Each change MUST be reviewed for conformance with this constitution and its approved specification.
Validation MUST be run at the smallest scope that proves the changed behavior; affected broader flows
MUST also be checked when the change crosses their boundary. Known deviations, incomplete validation,
and follow-up work MUST be recorded explicitly before delivery.

## Governance

This constitution supersedes conflicting project practices. Amendments MUST be proposed as a documented
change to this file, explaining the rationale, impact on existing work, and required migration or
follow-up actions. Versioning follows semantic versioning: MAJOR for incompatible principle changes,
MINOR for new or materially expanded principles or sections, and PATCH for clarifications that preserve
governance intent. Each change review MUST confirm compliance with these principles; exceptions require
an explicit, time-bound rationale approved by the project owner.

**Version**: 1.0.0 | **Ratified**: TODO(RATIFICATION_DATE): Initial adoption date is unknown. | **Last Amended**: 2026-09-24
