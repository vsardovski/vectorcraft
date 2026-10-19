# Тонекс print-shop edition (CorelDRAW-style features)

Status: prototype, 2026-10-19.

This fork adds what print-shop operators who come from CorelDRAW expect: its sign-making
commands, its default keys and its layout. Everything comes from public documentation and
general knowledge of how the app is used, under the same clean-room rules `CLAUDE.md` sets for
Adobe software:

- CorelDRAW is never run, scripted, screenshotted or measured to build or test this, and nothing is
  copied out of its install (binaries, presets, palettes, icons, help files).
- No Corel icons, artwork, palettes or wording beyond feature names and key names.
- User-visible strings say "the reference app" or "Print Shop", never the vendor's name
  (`cargo xtask brands` now rejects "corel" and "coreldraw").
- Reading `.cdr` files, if added later, goes through a clean-room or licence-compatible library
  (libcdr is MPL-2.0), never through decompiling the app.

## Sources

- Keyboard shortcuts: public community list,
  <https://community.coreldraw.com/collaborate/w/howto/8/complete-list-of-keyboard-shortcuts>
- Step and Repeat: Corel's online help, "Copy, duplicate, and delete objects",
  <https://product.corel.com/help/CorelDRAW/540111130/Main/EN/Documentation/CorelDRAW-Copy-duplicate-and-delete-objects.html>
- Cut contour spot colour name `CutContour`: the convention print-and-cut RIPs document publicly.

## What ships

| Feature | Where | Notes |
|---|---|---|
| Step and Repeat | Object › Print Shop, Ctrl+Shift+D (Print Shop keys) | No offset / Offset / Spacing between objects per axis, 1–1000 copies, one undo step |
| Add Cut Contour | Object › Print Shop | Spot `CutContour` hairline, overprinting, along the outline (offset) or a box |
| Convert to Curves | Object › Print Shop, Ctrl+Q | Type and live shapes to paths (`object.expand`, object only) |
| Combine / Break Apart | Object › Print Shop, Ctrl+L / Ctrl+K | Compound path make / release |
| One-key Align & Distribute | L R T B C E, P, Shift+letters | `printshop.align.*`, `printshop.distribute.*`, `printshop.centerToPage` |
| Print Shop Defaults key set | Edit › Keyboard Shortcuts › Set | F5–F10 tools, F11/F12 fill and outline panels, PgUp/PgDn order, Ctrl+I/E import/export |
| Function-key tools | `shortcuts.rs` | Tools may now take F1–F12 (they type no text, so need their own dispatch) |
| Color palette strip | Window › Color Palette | Right edge; click = fill, right-click = stroke, crossed chip = None |
| Fill/Stroke readout | Status bar, right end | Shown with the color palette |
| Print Shop workspace | Window › Workspace › Print Shop | Control bar as property bar, rulers, one-column toolbox, Layers docker, color palette |

## Not yet

- Corel's look itself (its colour scheme, icon set, docker title bars): only layout is matched.
- Interactive Contour, Envelope and Blend tools with Corel's property-bar controls.
- PowerClip editing as a mode (Ctrl+1 maps to Make Clipping Mask).
- Page tabs at the bottom (artboards are navigated from the status bar).
- `.cdr` import.
