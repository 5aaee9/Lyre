# Visual System

## Overview

Lyre is a product UI for live voice rooms. The current interface uses restrained system-responsive surfaces, Geist/system sans typography, Tailwind v4, and local shadcn-style primitives. Future design work should evolve toward Discord-like social immediacy while retaining a quiet, technical base for audio controls and diagnostics.

## Color Palette

- Light background: `#f6f8f5`, a soft green-tinted app shell.
- Dark background: `oklch(0.145 0.014 155)`, used only when `prefers-color-scheme: dark` is active.
- Foreground: `#18211c` in light mode and `oklch(0.94 0.01 155)` in dark mode.
- Panel surface: shadcn `--card` / `--popover`; never hard-code `bg-white` on app surfaces.
- Borders: use `--lyre-border` for panel edges and `--lyre-subtle-border` for internal dividers.
- Muted text: use `--lyre-muted-foreground`; keep muted text dark/light enough for WCAG AA on both system themes.
- Primary action: shadcn `--primary: oklch(0.205 0 0)` with `--primary-foreground: oklch(0.985 0 0)`.
- Destructive state: `--destructive: oklch(0.577 0.245 27.325)`.

State colors should be color-blind-safe and theme-aware through the `--lyre-success-*`, `--lyre-warning-*`, `--lyre-danger-*`, and `--lyre-muted-status-*` tokens. Do not encode speaking, muted, disconnected, or error states with hue alone; pair color with labels, icons, ring patterns, or explicit text.

## Typography

- Font family: Geist via `next/font/google`, exposed as `--font-sans`.
- Product UI should use one sans family across headings, labels, controls, and data.
- Type scale should stay compact and fixed, not fluid. Use clear weight and spacing before introducing larger display sizes.
- Headings should support quick scanning; labels and diagnostics should remain legible at dense product sizes.

## Layout

- Default posture: restrained, task-first product UI.
- Main pages use a centered `max-w-5xl` shell with a simple top header.
- Room surfaces should prioritize voice-room immediacy: participant presence, local audio state, and room controls before diagnostics.
- Diagnostics and detailed transport state should use progressive disclosure so the main room stays conversational rather than console-like.
- Responsive behavior should preserve control reachability on small screens and avoid hiding primary audio actions behind secondary menus.

## Components

- UI primitives live under `frontend/src/components/ui` and follow shadcn/Radix conventions.
- Theme colors live in `frontend/src/app/globals.css`; root, room, diagnostics, and settings surfaces should consume Tailwind token classes instead of fixed light hex values.
- Buttons use `class-variance-authority`, rounded `lg` corners, clear focus rings, and compact heights.
- Dialogs, selects, switches, and inputs should preserve the existing component vocabulary.
- Settings are local-browser product controls, not a marketing or onboarding surface.
- Participant controls should use recognizable icons from `lucide-react` when available, paired with accessible labels.

## Interaction

- Core room actions must be keyboard-first: join, leave, mute/unmute, settings, and per-user mute/volume.
- Focus states must be visible against both white panels and the green-tinted shell.
- Dropdowns and dialogs should keep Radix behavior and avoid clipped popups.
- Motion should be short and state-driven: connection changes, speaking state, and control feedback. Respect `prefers-reduced-motion`.

## Anti-patterns

- No generic SaaS card grids or marketing hero treatment for the app surface.
- No dense observability-console main room layout.
- No decorative gradient text, side-stripe card accents, or glassmorphism.
- No status color without a non-color companion.
- No diagnostics-first hierarchy unless the user explicitly asks for a debugging surface.
