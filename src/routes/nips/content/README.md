# Per-spec implementation notes

Drop a Markdown file here to ship rich implementation notes for a spec on the
`/nips/<route_id>` page. The filename is arbitrary, but by convention match the
route ID with an underscore (e.g. `nip_01.md` for `nip-01`).

After adding a file, flip the matching entry in `../registry.rs` from
`notes: None` to `notes: Some(include_str!("content/nip_01.md"))`.

Until an entry has `notes: Some(...)`, its detail page renders a stub with the
upstream link only.
