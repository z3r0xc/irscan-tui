# Design system — normative

This document specifies the visual system and the motion rules. It is normative: where
`docs/spec.md` says "a documented token system" (FR-19) and "motion with purpose" (FR-20),
this file is what that means, and the tests point at these sections.

The system descends from the design tokens in `prov/desktop/ui/style.css`, which was built
for exactly this product. The lineage is deliberate and the deviations are recorded below,
because a port that quietly invents a new system is not a port.

---

## 1. The one rule everything follows from

> **Severity is carried by glyph, weight and tag. Never by hue.**

The source states the reason: *this report gets pasted into tickets, projected, and read by
colour-blind reviewers*. A HIGH finding that is only red is invisible to a reader who cannot
distinguish red from orange, and the report outlives the screen it was read on.

So a severity is three simultaneous signals, any one of which is sufficient:

| Severity | Glyph | Weight | Tag | Meaning |
|---|---|---|---|---|
| HIGH | `!` | bold | `HIGH` | act on this now |
| MEDIUM | `~` | normal | `MED` | plausible, needs a decision |
| LOW | `.` | dim | `LOW` | noted |
| INFO | `·` | dim | `INFO` | context, not a finding |

Hue exists in this interface for exactly three jobs: **focus**, **accent** (the active
panel's edge), and **damage** (a rule that failed, a write that was refused). A reader who
sees a colour knows it means state, not severity.

## 2. Tokens

Ported from `ui/style.css:1-96` `:root`. Every value here has a name; no literal appears in
render code.

### 2.1 Surfaces — three, not six

| Token | Hex | Used for |
|---|---|---|
| `bg` | `#0a0a0a` | the content area |
| `bg_raised` | `#101010` | chrome, and only chrome: header and footer |
| `panel` | `#141414` | marks an object — never a band, never a region |

The distinction is load-bearing: a panel is *a thing* (a finding, a report, an archive
entry), a raised surface is *the frame around everything*. A banded layout would use a
fourth surface and lose that.

### 2.2 Lines

| Token | Hex | Used for |
|---|---|---|
| `line` | `#232323` | the single hairline that separates two regions |
| `line_strong` | `#2e2e2e` | a rule inside a panel, a table divider |

Regions are separated by **hairlines, not by fills**. Two adjacent raised fills with no rule
between them read as one region; that is the whole reason the rule exists.

### 2.3 Ink ramp — six steps, one job each

| Token | Hex | Job |
|---|---|---|
| `ink` | `#f6f6f6` | the value being read — a number, a verdict, a selected title |
| `ink_2` | `#bdbdbd` | body text: an evidence line, a remediation line |
| `ink_3` | `#8a8a8a` | supporting text: a category, a host fact, a timestamp |
| `ink_4` | `#565656` | a label, a column header, a key hint |
| `ink_5` | `#333333` | a disabled row, a placeholder, an unreachable control |
| `ink_void` | `#1c1c1c` | a filled meter track |

The ramp is a **single direction**: lighter means more important. There is no second hue in
the ramp, so two adjacent steps are always distinguishable as steps and never as categories.

### 2.4 Accent and damage

| Token | Truecolour | 256 | 16 | Used for |
|---|---|---|---|---|
| `accent` | `#5ac8fa` | `Indexed(45)` | `Cyan` | the focused panel's border and its title |
| `damage` | `#d75f5f` | `Indexed(167)` | `Red` | a failed collector, a refused delete, an unsaved change |
| `staged` | `#d7af5f` | `Indexed(179)` | `Yellow` | an edit that applies to the next scan |
| `good` | `#87af5f` | `Indexed(108)` | `Green` | a completed check, a resolved finding |

`staged` is the one warm colour with a job that matters: a staged rule edit must be
impossible to confuse with a saved one. It is the only place in the interface where "pending"
is communicated by colour rather than by text, and it is always accompanied by the text
`staged` — the colour is reinforcement, never the message.

### 2.5 Spacing — one scale, base 4

`1` = 4px = 1 cell · `2` = 8px · `3` = 12px · `4` = 16px · `6` = 24px

**No length in the interface is invented outside this scale.** A gap is `1` or `2`. A panel
padding is `1` on each side. If a layout needs a gap that is not on the scale, the layout is
wrong, not the scale.

### 2.6 Layout heights — reserved, not computed

The source's rule: *nothing below the status box may move when data arrives. Every region has
a reserved height and content that would exceed it scrolls inside it.*

| Region | Height | Grows? |
|---|---|---|
| header | 1 | no |
| tab bar | 1 | no |
| content | remainder | yes, this is the only one |
| status line | 1 | no |
| key hint | 1 | no |

This is the single most important layout property in the system and it is testable
(QR-7): render a screen with 0 findings and with 400, and the header, status and hint rows
are byte-identical.

## 3. Typography

The source has seven sizes. A terminal has **two reliable weights and one colour ramp**, and
a size change in a terminal is a glyph change, not a size change. Porting seven sizes
literally would produce seven unrelated glyph sets and a worse result, not a faithful one.

So the type scale collapses to three steps, each of which has exactly one job:

| Step | Realised as | Job |
|---|---|---|
| **display** | `ink` + `BOLD` | the one number you came to read: a verdict, a finding count, a host name |
| **body** | `ink_2`, normal | anything you read a word at a time: evidence, remediation, a log line |
| **label** | `ink_4` + `DIM` | a column header, a key hint, a unit, a count of something |

Plus two derived uses, which are not new steps but the display step applied in context:

- **machine string** — a path, a registry key, a command line, a SID. `ink_3`, normal, and
  always truncated by display width (§4.2). It is a distinct *kind* of content, not a
  distinct size: it is the thing that must never wrap, and it is styled so the eye finds it.
- **keycap** — a key hint, in `ink_4`, inside `bg_raised`. Rendered as `key` in the footer,
  and `[key]` in the help overlay.

**Line height is always 1.** No leading, no padding inside a line. Vertical rhythm comes from
the spacing scale and the reserved heights, never from leading.

## 4. Rendering rules

### 4.1 Borders

`Block::bordered().border_type(BorderType::Plain)` is the default. `Rounded` is used on the
focused panel only, so focus is legible without colour on a monochrome terminal.

A border is a **hairline in `line`**, never in a fill. The focused panel's border is
`accent`. A panel that is not focused has no title decoration, so a screen at rest has
exactly one bright thing on it.

### 4.2 Truncation — by display width, never by bytes

Machine strings are truncated with `unicode-width`, on a cell budget, to the remaining width
of the column. Not by `char` count: a path of CJK characters is fewer chars and twice the
cells, and a row that overflows wraps into the next row's layout and destroys the grid.

Where the tail of a path is the informative part, the head is elided instead
(`…\Windows\System32\…`) — the interesting end of a path is the end.

### 4.3 The severity chip

A finding row is: `glyph · weight · tag · title · … · evidence count · category`.

```
! HIGH   任务计划程序已创建持久化任务          3   autoruns
  ~ MED   未知远程工具服务名与已知工具匹配      1   remote_access
    LOW   偏好启动项指向用户可写目录            2   autoruns
```

The tag column is fixed width so the titles align. Titles are truncated to the remaining
width, never wrapped — a wrapped row breaks the one-row-per-finding scan that the whole
layout exists to support.

## 5. Motion

Motion is specified as **pure functions of elapsed time**. `ease(t: f64) -> f64` where
`t ∈ [0, 1]`. This is not a stylistic preference; it is what makes motion testable without a
terminal and without sleeping (QR-4).

### 5.1 Curves

| Name | Shape | Used for |
|---|---|---|
| `ease_out` | fast start, long settle | anything entering; a thing arriving should arrive, not slide |
| `ease_in_out` | symmetric | anything leaving and returning |
| `linear` | — | the progress bar only, because progress is a measurement, not a movement |

`ease_out` is the default and the others are exceptions. A UI where things *slide in* reads
as slow; a UI where things *arrive* reads as responsive.

### 5.2 The four motions, and only these four

| Motion | Duration | Applies to | Why it exists |
|---|---|---|---|
| **cross-fade** | 140 ms | a screen change | marks a transition without making the user wait for it |
| **reveal** | 180 ms, staggered 12 ms per row, max 10 rows then all at once | a list gaining items | a new finding is distinguishable from a row that was always there |
| **meter** | linear, 100 ms per 1% | a progress bar | shows *rate*, which is the thing that distinguishes "working" from "hung" |
| **spinner** | 8 frames @ 90 ms | an indeterminate wait | only while something is genuinely unknown-duration |

Anything else — parallax, bounce, elastic, typewriter — is not used. A triage tool is used
under time pressure by someone reading evidence, and motion that draws the eye is motion
that costs them a finding.

### 5.3 Stagger has a cap

Rows past the 10th appear together. A 400-finding list that staggers for 4.8 seconds is a
list that appears broken, and the user is reading the *top* of it anyway.

### 5.4 Motion is switchable, and it is off in the degraded paths

`--no-motion`, or `NO_COLOR` set, or not a TTY, and every duration above becomes 0 — the
motion still *happens*, it is just instantaneous, so no code path has a "skip the animation"
branch to get wrong. This is why motion is expressed as a duration rather than as a boolean.

### 5.5 Frame policy

30 fps, a redraw only when something changed. The tick advances animations; it does not
schedule a draw on its own. A still screen costs nothing, which is what lets a long-running
scan sit in the background without making the terminal hot.

## 6. Colour degradation

**Ratatui 0.30 has no colour-depth API.** There is no `ColorDepth`, no `with_color`, and no
RGB→256 quantisation; the backend maps `Color` to crossterm 1:1 and clamps nothing. Detection
is the application's job, and the portable mechanism is the environment:

| Signal | Result |
|---|---|
| `NO_COLOR` set | none |
| `COLORTERM` = `truecolor` or `24bit` | truecolour |
| `TERM` contains `direct` or `truecolor` | truecolour |
| `TERM` contains `256color` | 256 |
| `TERM` = `dumb`, or unset | 16 |

The palettes in §2.4 are declared **twice**, as truecolour and as 256/16, and one whole
`Theme` is selected at startup and held in `App`. There is no per-frame quantisation: a
`Color` is four bytes and a `Style` is five of them, so re-deriving a theme per frame is
work for nothing. **No render code branches on depth.**

### 6.1 What changes, and what must not

| Depth | Changes |
|---|---|
| truecolour | full system as specified |
| 256 | §2.4's `Indexed` column; all other structure identical |
| 16 | `accent`/`damage`/`staged`/`good` collapse to Cyan/Red/Yellow/Green; **`DIM` is replaced by `BOLD` on a dark ink step**, because many terminals render `DIM` as near-invisible and the label step would vanish |
| none | no `fg`/`bg` anywhere; every distinction above survives through **glyph, weight and text**, because §1 was written to make it survive this |

The `none` case is the test of the whole system. If the interface still communicates severity,
focus and state in monochrome, then the colour was never carrying the information — it was
reinforcing a structure that already works. `QR-6` requires a test per depth that asserts
what the user actually sees.

## 7. What was dropped in the port, and why

Honesty about the losses, because a port that claims fidelity it does not have is worse than
one that names its gaps.

| Source | Dropped | Why |
|---|---|---|
| 7 type sizes | 3 steps | A terminal has no size axis; 7 unrelated glyph sets is worse than 3 coherent ones (§3) |
| `--sp-*` 4→32px | 1→6 cells | Cells are already the unit; sub-cell spacing is not expressible |
| Weight 600 for HIGH | `BOLD` | The only weight that exists |
| Variable font stacks | — | The terminal's font is not ours to choose. Unicode box-drawing and block glyphs are assumed present; a `--ascii` flag substitutes `+-|` and `.` for them, for terminals that cannot render them |
| Suspended layout heights in px | 1 cell per region | Same reason |

The system is *narrower*. It is not weaker: every rule that carried meaning survived, because
the meaning was never in the pixels.

## 8. Pointers

| Concern | Where |
|---|---|
| Requirements this implements | `docs/spec.md` FR-19, FR-20, FR-21, QR-6, QR-7, SR-1, SR-2 |
| Source tokens | `prov/desktop/ui/style.css:1-96` |
| Source layout discipline | `prov/desktop/src/view.rs`, reserved heights in the header/status/footer |
| Severity semantics | `ir-recon/src/model.rs:16-42` (`Severity::tag`, `Severity::label`) |
