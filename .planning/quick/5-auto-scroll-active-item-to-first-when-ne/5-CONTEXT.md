# Quick Task 5: Auto-scroll active item to first when new clipboard item arrives - Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

<domain>
## Task Boundary

When the active item is at the first position in the clipboard list, and a new item arrives (prepended), the active selection should automatically follow to the new first item. Currently, the active stays on the old item which shifts to index 1.

</domain>

<decisions>
## Implementation Decisions

### Trigger Condition

- Only auto-follow when the currently active item is the FIRST item in the list
- If user has selected any other item (index > 0), keep their selection stable

### Event Source

- Consistent behavior for both local clipboard changes and remote device sync
- Both pathways should trigger the auto-follow logic when conditions are met

### Scroll Behavior

- After following to the new first item, automatically smooth-scroll to the top to ensure visibility
- Leverages existing scroll-to-active-item mechanism already in ClipboardContent

</decisions>

<specifics>
## Specific Ideas

- The fix is in `ClipboardContent.tsx`'s `useEffect` that manages `activeItemId`
- Current logic: only auto-selects first item when `activeItemId` is null or item not found in list
- Needed: detect when active was at index 0 before list update, and if so, re-select the new first item
- Both local (prependItem) and remote (full reload) paths result in flatItems changing, so the fix at the component level covers both

</specifics>
