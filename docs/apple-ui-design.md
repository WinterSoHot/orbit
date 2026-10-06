# Orbit — macOS-inspired interface

Brief: a personal Agent desktop workspace that feels calm, clear, and familiar on a Mac. Existing execution, persistence, synchronization, and Markdown editing remain the same.

## Visual direction

- Canvas `#F5F5F7`; sidebar `#ECEDEF`; paper `#FFFFFF`; primary text `#1D1D1F`; secondary text `#62626B`; interaction blue `#007AFF`.
- System typography: `-apple-system`, BlinkMacSystemFont, Helvetica Neue, PingFang SC. Body 13–14 px, task title 18 px, page title 25 px. Left aligned; readable secondary text rather than miniature pale labels.
- Status colors convey execution state; blue conveys selection and primary action. Border radii follow function: compact controls 6–8 px, task surfaces 10–12 px, modal sheets 16 px. Shadows only for floating sheets and selected graph nodes.

```text
Sidebar        Workspace toolbar                 New task
               Title and compact status summary
Tasks/scenes   Task list       Agent collaboration    Inspector
               Search/filter  Run metadata            Selected Agent
                              Recent activity / delivery
Settings
Local status
```

Alternative considered: a decorative glass dashboard with large status cards. Chosen: a utility workspace, with solid readable surfaces, restrained translucency in chrome, and the actual Agent graph as the focal point. Removed the promotional hero and separate metric cards because they compete with the current task. No decorative traffic-light controls or nonfunctional menus.

## Implementation and verification

1. Adjust existing App markup and product copy; retain handlers and state contracts. Replace metric cards with compact summary links and remove fake profile/menu affordances.
2. Replace the existing stylesheet with one token-based system covering navigation, task list, graph, inspector, collections, forms, and Markdown sheets. Keep responsive layouts and visible keyboard focus; reduce motion.
3. Review browser screenshots at desktop and narrow sizes, task/node selection, navigation, create sheet, and Markdown sheet. Run the existing frontend checks and type/build checks; rebuild Orbit.app.
4. Read-only architect C review of the final incremental UI diff before completion. Native App interaction remains unavailable to automation; browser preview provides visual verification.

This is a bounded frontend redesign, not a large cross-module execution or runtime architecture plan. No backend, dependencies, model calls, or new scheduling behavior are included.
