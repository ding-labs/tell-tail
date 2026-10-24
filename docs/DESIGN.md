# Visual foundation

Tell-Tail uses the angular log-tail mark: three log lines, with the lowest
continuing into an upright, angled tail. `web/logo.svg` is the clean, transparent
vector source in original Ding green (`#7EE787`). `web/favicon.svg` places the
same geometry on graphite so it stays legible against light and dark browser
chrome. The dashboard gives the mark the same graphite backing.

## Dashboard palette

The color values and semantic roles come from Ding's `design/tokens.css` and
`design/brand.md`. They are mirrored as `--ding-*` properties at the top of
`web/dashboard.css`, keeping the embedded dashboard self-contained and offline.
The browser's color preference selects light or dark; changes apply immediately.

| Role | Light | Dark |
| --- | --- | --- |
| Canvas | `#F7F6F2` | `#171C19` |
| Surface | `#FFFFFF` | `#202722` |
| Subtle surface | `#EEEEE7` | `#29312B` |
| Text | `#222B25` | `#F0F1E9` |
| Muted text | `#62695F` | `#ACB6AB` |
| Border | `#D9DDD2` | `#3B473C` |
| Brand, links, throughput, focus | `#246239` | `#7EE787` |
| Active control surface | `#E3F3DF` | `#2A3C2C` |
| Information and unknown | `#68538B` | `#C2ADDF` |
| Information surface and selection | `#ECE5F3` | `#332C40` |
| Warning | `#80530F` | `#EBBC74` |
| Warning surface | `#FBF1DD` | `#362D1E` |
| Error and fatal | `#A33C33` | `#F0A398` |
| Error surface | `#F9EAE7` | `#3D2826` |

Keep bright original green in the logo. Use pine for readable green text and
charts on light surfaces. Violet marks the live connection because connectivity
does not imply healthy input. Warning and error ratios use their severity colors;
error and fatal share coral and remain distinguished by text. Trace/debug and
inactive or final states use neutral colors. Selection and keyboard focus remain
visible in both schemes.

This first design pass changes the mark and colors. The dashboard's type scale,
spacing, component shapes, responsive layout, and behavior retain their existing
structure for later design work.
