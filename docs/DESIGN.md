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

## Typography

The dashboard follows Ding Console's system sans-serif and monospace stacks,
with a 14 px body at 1.6 line height, 29 px page headings (25 px on mobile),
21 px section headings, and 14 px metric labels. Supporting text is 12 px and
eyebrows are 11 px. Paths and log previews use the shared monospace stack;
counters use tabular numerals. Metric values are 32 px (28 px on mobile), with
600 weight for a clear hierarchy. Fonts remain local and work offline.

## Controls

Buttons use Ding's 6 px control radius, 13 px system text, quiet bordered
surfaces, and 8 × 12 px padding, with a 40 px minimum height. Freeze/resume and
fatal filtering retain pressed-state feedback; retry uses original Ding green
with dark text as the recovery action. Hover, disabled, and keyboard focus
states follow the same tokens. Native disclosures keep their keyboard behavior,
with a green text action for exact chart values. Focus rings are 2 px with a
3 px offset, including on the scrollable error list.

## Spacing and panels

Spacing follows Ding's 4 px grid, using 8/12/16/24/32 px steps. Source, metric,
and detail panels share a flat bordered surface, 12 px corners, and 24 px inner
padding (16 px on mobile). Major sections use 24 px gaps; metric cards use
16 px gaps. Page gutters step from 32 to 24 to 16 px with the viewport.

Table headings use muted uppercase labels on subtle surfaces, with consistent
row padding and a quiet hover state. Panel headings wrap on narrow screens;
source paths, counters, and log previews can wrap without widening the page.
Diagnostics retain a native disclosure with a padded full-width header and
an inset grid. The chart's geometry, animation, data flow, and every existing
control remain unchanged.

## Terminal palette

`src/tui/theme.rs` mirrors Ding's dark colors: graphite canvas and panel surfaces,
warm text, green throughput, violet information/unknown records, amber warnings,
and coral errors/fatals. Trace and debug records use muted text. Severity names
remain visible, including the distinction between error and fatal.

In color mode, the terminal paints its own dark canvas so the palette is readable
on light and dark terminal profiles. It respects `NO_COLOR` and uses the terminal's
font and existing cell geometry; the alternate screen and terminal settings are
restored on exit.

The full header pairs a green `≡╱` angular log-tail text mark with a bold
lowercase wordmark. Rounded box-drawing borders, bold panel titles, and a subtle
table heading provide hierarchy within the existing cell layout. Reading and
following states are violet, waiting is amber, failures are coral, and frozen
or final states are neutral. Compact views keep their seven-line summary and
use the same wordmark and severity colors. All keyboard shortcuts, review
limits, chart history, and refresh behavior are preserved.
