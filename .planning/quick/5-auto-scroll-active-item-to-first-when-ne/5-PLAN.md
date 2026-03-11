---
phase: 5-auto-scroll-active-item
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - src/components/clipboard/ClipboardContent.tsx
autonomous: true
requirements:
  - QUICK-5
must_haves:
  truths:
    - 'When active item is at index 0 and a new item arrives, active selection moves to the new first item'
    - 'When active item is NOT at index 0, selection stays on the same item after new item arrives'
    - 'Scroll follows the newly-activated first item to keep it visible'
  artifacts:
    - path: 'src/components/clipboard/ClipboardContent.tsx'
      provides: 'Auto-follow logic for first-position active item'
      contains: 'wasAtFirstPosition'
  key_links:
    - from: 'src/components/clipboard/ClipboardContent.tsx'
      to: 'flatItems[0]'
      via: 'useRef tracking previous first-position state'
      pattern: 'wasAtFirstPosition'
---

<objective>
Add auto-follow behavior so that when the active item is at position 0 in the clipboard list and a new item is prepended, the active selection automatically moves to the new first item.

Purpose: Users viewing the latest clipboard item expect to keep seeing the latest — not have it shift down when a new capture arrives.
Output: Updated ClipboardContent.tsx with ref-based first-position tracking.
</objective>

<execution_context>
@/Users/mark/.claude/get-shit-done/workflows/execute-plan.md
@/Users/mark/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@src/components/clipboard/ClipboardContent.tsx
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add auto-follow logic for first-position active item</name>
  <files>src/components/clipboard/ClipboardContent.tsx</files>
  <action>
In ClipboardContent.tsx, add a `useRef<boolean>` called `wasAtFirstPositionRef` to track whether the active item was at index 0 before `flatItems` changed.

1. Add ref after the existing `activeItemRef`:

   ```ts
   const wasAtFirstPositionRef = useRef(false)
   ```

2. Add a useEffect AFTER the `activeIndex` memo (around line 195) to keep the ref in sync with the current position:

   ```ts
   useEffect(() => {
     wasAtFirstPositionRef.current = activeIndex === 0
   }, [activeIndex])
   ```

3. Modify the existing "Auto-select first item" useEffect (lines 204-214) to also handle the auto-follow case. Add a condition: if `wasAtFirstPositionRef.current` is true AND `flatItems[0]` exists AND `flatItems[0].id !== activeItemId`, then set activeItemId to `flatItems[0].id`. This condition should be checked BEFORE the existing null/not-found check. The full useEffect becomes:

   ```ts
   useEffect(() => {
     if (flatItems.length > 0) {
       // Auto-follow: if active was at first position, follow the new first item
       if (wasAtFirstPositionRef.current && flatItems[0].id !== activeItemId) {
         setActiveItemId(flatItems[0].id)
         return
       }
       // Auto-select: if no active item or active item no longer in list
       if (activeItemId === null || !flatItems.some(it => it.id === activeItemId)) {
         setActiveItemId(flatItems[0].id)
       }
     }
     if (flatItems.length === 0) {
       setActiveItemId(null)
     }
   }, [flatItems, activeItemId])
   ```

This covers both local (prependItem) and remote (full reload) paths since both result in flatItems changing. The existing scroll-to-active useEffect (line 217-219) will automatically smooth-scroll to the new active item.
</action>
<verify>
<automated>cd /Users/mark/MyProjects/uniclipboard-desktop && npx tsc --noEmit --pretty 2>&1 | head -30</automated>
</verify>
<done> - TypeScript compiles without errors - wasAtFirstPositionRef is declared and updated via useEffect - Auto-follow logic fires when active was at index 0 and flatItems[0] changes - Existing auto-select behavior preserved for null/not-found cases - No behavior change when active item is NOT at index 0
</done>
</task>

</tasks>

<verification>
1. TypeScript compilation passes with no errors
2. Manual test: open app, active item is first item, copy something new to clipboard — active should jump to the new first item
3. Manual test: select a non-first item, copy something new — selection should stay on the same item
</verification>

<success_criteria>

- Active selection auto-follows new first item when user was viewing the first item
- Active selection remains stable when user has selected a non-first item
- Smooth scroll to top occurs when auto-follow activates
- Both local clipboard capture and remote sync trigger the auto-follow correctly
  </success_criteria>

<output>
After completion, create `.planning/quick/5-auto-scroll-active-item-to-first-when-ne/5-01-SUMMARY.md`
</output>
