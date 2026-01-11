# Developer Note: React Component Nesting & Input Loss

## The Issue
We encountered a bug where dragging a slider or color picker in the `SettingsDialog` would only work for a split second (or a single step) and then lose focus/interrupt the drag.

## The Cause
The `SettingsDialog` component defined a helper component (`SectionCard`) *inside* its own function body.

```tsx
// ❌ BAD PATTERN
function SettingsDialog(...) {
  // This component is redefined on every render of SettingsDialog
  const SectionCard = ({ children }) => <div>{children}</div>;

  return (
    <SectionCard>
      <input type="range" ... />
    </SectionCard>
  );
}
```

When the state (e.g., slider value) changed, `SettingsDialog` re-rendered. This caused `SectionCard` to be redefined as a *new* component type. React then unmounted the old `SectionCard` (and its children, including the focused input) and mounted a new one. This unmounting/remounting cycle destroyed the DOM element that had focus and was capturing pointer events, thus breaking the drag operation immediately.

## The Fix
Move helper components *outside* the main component function.

```tsx
// ✅ GOOD PATTERN
const SectionCard = ({ children }) => <div>{children}</div>;

function SettingsDialog(...) {
  return (
    <SectionCard>
      <input type="range" ... />
    </SectionCard>
  );
}
```

## Related: Window Dragging
We also added `style={{ WebkitAppRegion: 'no-drag' }}` and `onMouseDown={(e) => e.stopPropagation()}` to inputs and the modal container. While important for preventing the window from being dragged instead of the slider (because of `data-tauri-drag-region` or similar backend logic), this was *not* the primary cause of the "single step" interruption. The primary cause was the component re-definition.
