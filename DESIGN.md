# Design

<!-- impeccable:design-schema 1 -->

Direction for nilos-reviewer: quiet craft with controlled personality.
The incumbent flat card language stays; life comes from precision,
depth, and deliberate expressive moments — not decoration.

## World

- **Character:** calm, dense, authored — a tool for people who read
  queues all day. Restraint is the baseline; personality appears only
  where it earns its place (the action surface, state changes).
- **Material:** flat cards and panels, hairline borders, soft shadows
  for depth. No skeuomorphism — buttons are solid fills with depth
  from color and border, never fake 3D.
- **Energy:** controlled. The UI should feel *still* at rest: no ambient
  animation, no decorative effects. Motion belongs to feedback — press,
  submit, advance — never ornament.

## Signature accent

The app carries its own accent identity on top of the Omarchy/live
palette: one consistent accent hue for interactive primary affordances
(accent color in Kit), with semantic colors fixed to action meaning —
green approve, red changes, amber blocked — regardless of palette.
Must read well on any Omarchy palette and on default macOS dark/light.

## Affordance rule (the button/badge collision)

If it acts, it looks like it acts. If it informs, it stays quiet.

- **Actions** are filled, weighty, unmistakably interactive — composer
  buttons get the expressive budget (solid fills, real borders, weighty type).
- **Metadata** (reason badges, state chips, counts) stays hollow or
  subtle — border or tinted text only, never filled the same way a
  button is.
- Test: squint — buttons must pop off the row while badges recede.

## Layout & rhythm

- **More air:** rows and groups get breathing room — taller row padding,
  group headers separated from their rows, consistent vertical rhythm.
- Two panes + draggable divider; narrow under ~780pt becomes
  single-pane queue/detail.
- Composer is a distinct action zone at the detail's foot: solid-fill
  buttons, shortcut hint text, no other competing chrome.

## Typography

Hierarchy through size + weight, not color alone: titles medium,
metadata small muted, badges tiny. Row metadata (#num · author · age)
sits at `text_muted` minimum — never `text_faint` for readable content.

## State language

- Submitting: inline spinner on the row, buttons disabled.
- Blocked: warning octagon + hidden by default.
- Handled: muted + tick, behind "Done" toggle.
- Selected: accent-tinted row fill (14% alpha).
- Checks: aggregated badge — failing/pending/passing counts with names
  in the tooltip.

## What stays banned

- Liquid metal, mesh gradients, fluted glass, glow/pulse as decoration.
- Ambient animation — the compositor returns `animating() = false`
  unless something is genuinely moving.
- Bevels/3D anywhere.
