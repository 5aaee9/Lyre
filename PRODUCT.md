# Product

## Register

product

## Users

Lyre is for people who need to enter a voice room quickly, stay aware of who is present, and control their own audio without wrestling with conferencing setup. Users are in a live conversation context: they need fast room entry, clear connection state, understandable microphone and playback controls, and recoverable settings when devices or relay state change.

## Product Purpose

Lyre provides high-performance VOIP rooms backed by a Rust server relay, server-side noise cancellation, and a focused Next.js room interface. Success means joining a room feels as immediate as a Discord-style voice channel while the interface stays calm enough for debugging real audio state when needed.

## Brand Personality

Direct, social, technically credible. Lyre should feel like a voice room product first: familiar, responsive, and channel-oriented, with enough engineering clarity to expose relay, noise, and device controls without turning the UI into an operations console.

## Anti-references

Avoid generic SaaS landing-page polish, decorative dashboard cards, dense observability-console layouts, and enterprise admin styling. Do not make the product feel like a metrics cockpit or a form-heavy configuration tool. The target reference direction is Discord-like room immediacy and social presence, not a Slack clone, generic collaboration SaaS, or monitoring dashboard.

## Design Principles

- Make voice state visible at a glance: presence, speaking, muted, reconnecting, failed, and device state should be readable without opening diagnostics.
- Keep primary room controls keyboard-first and reachable: join, leave, mute, settings, and per-user audio controls must support fast focus movement and clear focus states.
- Separate everyday conversation from diagnostics: debugging data belongs behind intentional disclosure, not in the main room flow.
- Prefer familiar voice-chat affordances over invented controls: room lists, participant rows, microphone toggles, volume controls, and status chips should behave predictably.
- Use color plus shape, text, or iconography for state so status never depends on hue alone.

## Accessibility & Inclusion

Target WCAG AA contrast for text, controls, and focus indicators. Status indicators must be color-blind-safe by pairing hue with labels, icons, shape, or motion-independent changes. Core room control should be keyboard-first, with visible focus states, logical tab order, and no reliance on hover-only affordances. Motion should respect reduced-motion preferences.
