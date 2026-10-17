# <Feature name>

- Status: ready-for-implementation
- Spec: <Outline/Notion URL or "none">
- Date: YYYY-MM-DD
- Branch: agent/<slug>

## Goal
One paragraph: what changes for the user and why.

## Context
Research findings that affect implementation, with source links.
Decisions already made and the alternatives rejected, with reasons.

## Scope
- In: ...
- Out (do not touch): ...

## Tasks
1. [ ] <task>. Acceptance: <observable result>
2. [ ] ...

## Verification
All checks must run in the background. Never take over the screen, cursor, or keyboard focus.
- Web: `npx playwright test <spec>` (headless)
- iOS: `xcodebuild test -scheme <Scheme> -destination 'platform=iOS Simulator,name=<Device>'`
- macOS: unit and snapshot tests only
- Other: <command>

## Open questions
Stop and ask before deciding on these.
