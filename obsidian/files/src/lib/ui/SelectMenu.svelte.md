---
tags: [file, frontend, ui, accessibility]
---
# `src/lib/ui/SelectMenu.svelte`

Shared controlled listbox. Its content and click-away backdrop are portalled to
`document.body`, positioned against the trigger with viewport collision
handling, and styled by the global menu/control tokens. It supports
Arrow/Home/End, Enter/Space, Escape, Tab, selected/disabled states, focus
restoration, and reduced motion.

## See also

[[app.css]] · [[AudioOutputSelector.svelte]] · [[Settings.svelte]]
