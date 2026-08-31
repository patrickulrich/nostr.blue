# NIP-101e — Fitness Workouts

nostr.blue implements the NIP-101e (draft) fitness workouts layer.
Kind-1301 events are published in two structurally distinct wire forms,
which nostr.blue distinguishes without any marker tag:

## Kind 1301 — Workout Record

- **Activity form** (what nostr.blue publishes): `d` (UUID), `exercise`
  (plain verb, e.g. `running`), capitalized `t` hashtag (e.g.
  `Running`), `duration` (`HH:MM:SS` or raw seconds), plus optional
  `title`, `source` (`gps` | `manual` | `health_connect`), `distance`
  (value + `km`/`mi`/`m`), `calories` (kcal), `avg_heart_rate` /
  `max_heart_rate` (bpm), `steps`, `elevation_gain` / `elevation_loss`
  (value + `m`/`ft`), `split` (index + cumulative time), totals `sets` /
  `reps` / `weight` (`lbs`/`kg`), and `workout_start_time` (unix
  seconds). The content is plain-text user notes, never JSON.
- **Strength form** (rendered, not published): `type` (`strength` |
  `circuit` | `emom` | `amrap`), `start` / `end` / `completed` session
  timestamps, a `template` coordinate (kind 33402), and per-set
  `exercise` tags carrying a kind-33401 coordinate plus weight (kg),
  reps, RPE, set type, and set number. The two forms share the
  `exercise` tag name and are distinguished structurally: a coordinate
  (`33401:<pubkey>:<d-tag>`) marks the strength form.

On read, nostr.blue prefers the strength-form `type` classification and
falls back to the activity verb; duration prefers the explicit
`duration` tag and falls back to `end − start`. Workouts with distance
render pace (cycling renders speed) and convert distance/elevation/
weight to the viewer's preferred units (metric/imperial, from settings
or locale).

## Kind 33401 — Exercise Template

Addressable definitions: `title`, `format` (ordered per-set parameter
names), `format_units`, `equipment`, and `difficulty`. nostr.blue
fetches these by coordinate (gossip-routed to the template author's
write relays) to render real exercise names in strength-workout
breakdowns, falling back to the prettified d-tag slug until resolved.

## Surfaces

- `/workouts` feed (Following / Global tabs, one-week lookback on
  Global), `/workouts/new` composer, and kind-aware detail views via
  `nevent`/`naddr` links.
- Android: workouts recorded in Health Connect (Samsung Health, Google
  Fit, Fitbit, Garmin, Strava, …) are offered as pre-filled posts in
  the composer; close-by sessions of the same type are merged
  (per-type gap chaining under one hour, duration-weighted average
  heart rate). Permission is requested only when the user taps
  Connect, and is read-only.

## References

- NIP-101e (draft): kind 1301 workout records and kind 33401 exercise
  templates. These implementation notes serve as the spec reference
  for the wire forms nostr.blue reads and publishes.
