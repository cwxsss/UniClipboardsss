---
name: gsd:do
description: Route freeform text to the right GSD command automatically
argument-hint: '<description of what you want to do>'
allowed-tools:
  - Read
  - Bash
  - AskUserQuestion
---

<objective>
Analyze freeform natural language input and dispatch to the most appropriate GSD command.

Acts as a smart dispatcher — never does the work itself. Matches intent to the best GSD command using routing rules, confirms the match, then hands off.

Use when you know what you want but don't know which `/gsd:*` command to run.
</objective>

<execution_context>
@/Volumes/ExternalSSD/myprojects/uniclipboard-desktop/.claude/get-shit-done/workflows/do.md
@/Volumes/ExternalSSD/myprojects/uniclipboard-desktop/.claude/get-shit-done/references/ui-brand.md
</execution_context>

<context>
$ARGUMENTS
</context>

<process>
Execute the do workflow from @/Volumes/ExternalSSD/myprojects/uniclipboard-desktop/.claude/get-shit-done/workflows/do.md end-to-end.
Route user intent to the best GSD command and invoke it.
</process>
