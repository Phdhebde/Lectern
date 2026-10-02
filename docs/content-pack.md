# Content pack format (version 1)

The platform is published without content. Tracks, modules, scenarios, question banks
and certification requirements are packaged as a **content pack**: a directory (or a
`.zip` of it) of TOML, Markdown and image files, kept in the instance's own (private)
repository and reviewed like code.

- Import: `lectern import-pack <dir|zip>` or *Administration › Content › Import a pack*.
- Export: `lectern export-pack out.zip` or *Administration › Content › Export content*.
  An export is a valid pack: export, edit, re-import.

A fictional example lives in [`examples/demo-pack`](../examples/demo-pack).

## Layout

```
pack.toml                                   format version, requirement levels
tracks/<track>/track.toml                   track settings, badge, exam definitions
tracks/<track>/modules/<NN>-<module>.md     one module: TOML front matter + recap sheet
tracks/<track>/files/…                      module attachments
tracks/<track>/questions/*.toml             question banks (any file name)
tracks/<track>/scenarios/<scenario>/scenario.toml   scenario + its questions
tracks/<track>/scenarios/<scenario>/*.png           screenshots of that scenario
```

Identifiers (`<track>`, `<module>`, `<scenario>`) are lowercase slugs (`a-z`, `0-9`, `-`).
Module order comes from the numeric prefix of the file name (`01-…`, `02-…`).

## Import rules

- **Idempotent.** Tracks, modules and scenarios are matched by slug, questions by `ref`.
  Re-importing the same pack changes nothing; importing a new version updates in place.
- **A pack is authoritative for the tracks it contains.** Modules and scenarios missing
  from the pack are deleted (with the related progress); questions missing from the pack
  are *deactivated*, never deleted, so past exam papers stay readable. Tracks absent from
  the pack are left untouched.
- **All or nothing.** The import runs in one transaction; any validation error aborts it
  and reports the file and the problem.
- Files are content-addressed (SHA-256): identical screenshots are stored once.

## `pack.toml`

```toml
format = 1
name = "Partner academy content"

# Requirement levels: how many valid certified people each kind of organization needs,
# per track, to hold a level (e.g. partner tiers). Generic: any names, any number.
[[levels]]
slug = "partner-silver"
org_kind = "partner"        # partner | customer
name = "Silver"
rank = 1                    # order of the levels
requirements = { associate = 1, engineer = 2 }
```

## `track.toml`

```toml
title = "Engineer"
summary = "Deploy, administer and provide first and second level support."
description = """Markdown shown on the track page."""
audiences = ["partner"]     # any of: public, partner, customer
position = 3                # order in the catalog
prerequisite = "engineer"   # optional: slug of a track whose valid certification is required to sit the exam
prerequisites = "Markdown text describing the expected background."
estimated_minutes = 720
scenarios_required = true   # all (non exam-only) scenarios must be completed before the exam
validity_months = 24        # omit for certifications that never expire
module_quiz_pass_percent = 70
published = true

[badge]                     # all optional
label = "ENGINEER"          # big text (defaults to the title)
subtitle = "Certified"      # ribbon
color = "#7c3aed"           # defaults to the theme primary colour
accent = "#f59e0b"          # defaults to the theme accent colour

[exam]
free_attempts = 2           # omit for unlimited attempts
cooldown_days = 14          # wait between attempts once the free ones are used
bank_factor = 3             # the bank must hold at least 3× the questions drawn
provisional = false         # certifications issued are provisional (see below)

[[exam.sections]]
title = "Multiple-choice"
kind = "questions"
pool = "exam"               # exam | recert
question_count = 40
duration_minutes = 60
pass_percent = 75

[[exam.sections]]
title = "Case study"
kind = "case_study"
scenario_count = 2          # exam-only scenarios drawn; at least 2× this many must exist
duration_minutes = 90
pass_percent = 75           # ignored when the section only has written questions

[recert_exam]               # optional short exam on what's new; omit to reuse [exam]
bank_factor = 3
[[recert_exam.sections]]
title = "What's new"
kind = "questions"
pool = "recert"
question_count = 10
duration_minutes = 20
pass_percent = 75
```

Sections are taken one after the other, each with its own timer. The exam stops at the
first failed section. A section containing written questions goes to an evaluator
(attempt status *pending review*) and is decided manually.

**Provisional certifications.** During a transition period (e.g. while case studies are
being written), publish a reduced exam with `provisional = true`. Holders of a
provisional certification take the full `[exam]` at their recertification.

## Modules: `modules/<NN>-<slug>.md`

```markdown
+++
title = "Architecture"
video = "https://media.example.com/engineer/architecture/index.m3u8"   # MP4 or HLS
captions = "https://media.example.com/engineer/architecture/fr.vtt"    # WebVTT
duration_minutes = 9
doc_url = "/architecture"     # relative to instance.documentation_url, or absolute
attachments = [{ file = "files/architecture.pdf", label = "Architecture diagram" }]
+++

Recap sheet in **Markdown** (tables, lists, code blocks…).
```

A module is completed when its content is viewed (video watched to the end, or sheet
marked as read) and, if it has a quiz, the quiz reaches `module_quiz_pass_percent`.

## Questions: `questions/*.toml`

```toml
[[questions]]
ref = "eng-arch-001"        # stable unique identifier across the whole instance
pool = "exam"               # quiz | exam | recert | case
module = "architecture"     # quiz questions: the module they close
prompt = """Which flow does the gateway agent need?"""
explanation = "Shown after a quiz; never shown during or after an exam."
choices = [
  { text = "Outbound 443/tcp", correct = true },
  { text = "Inbound 3389/tcp" },
  { id = "c", text = "Inbound 22/tcp" },   # ids default to a, b, c…
]
active = true
```

- Several correct choices are allowed; an answer is right only if it selects exactly
  the correct set. Learners always see checkboxes, so the number of correct answers is
  not revealed.
- `format = "written"` (no choices) asks for a free-text answer graded by an evaluator.
- Pools: `quiz` (module or scenario check, corrected immediately), `exam`, `recert`
  (certification banks, answers never revealed), `case` (questions of an exam-only scenario).
- Write prompts and choices so that they make sense in any order: choices are shuffled.

## Scenarios: `scenarios/<slug>/scenario.toml`

```toml
title = "Gateway offline"
kind = "diagnostic"        # implementation | diagnostic
family = "gateway-offline" # troubleshooting family of the documentation
exam_only = false          # true: case study for exams, never shown in the track
position = 2
context = "Users cannot open any session. The console shows the gateway offline."
pitfalls = "Restarting the agent without checking the outbound flow only hides the problem."

[[steps]]
action = "Observe the gateway status and its last-seen time."
image = "step-1.png"        # file in this directory (PNG, JPEG, WebP)
alt = "Gateway shown offline"
expected = "The gateway is offline since 10:42: something changed at that time."
annotations = [
  { type = "box", x = 25, y = 22, w = 50, h = 14, label = "1" },
  { type = "arrow", x = 70, y = 70, x2 = 85, y2 = 80, label = "2" },
  { type = "marker", x = 27, y = 29, label = "3" },
]

[[questions]]               # pool "quiz" = check at the end; "case" = exam question
ref = "eng-sc-offline-1"
pool = "quiz"
prompt = "What was the cause?"
choices = [{ text = "Outbound 443/tcp blocked", correct = true }, { text = "Expired licence" }]
```

Annotation coordinates are **percentages** of the image (0–100), so they survive any
resizing. Their colour is the theme token `color-annotation`. The annotation editor in
*Administration › Content › (track) › Scenarios* produces these values with the mouse.

Screenshots must come from a demo tenant with **fictitious data only** — no real server
name, address or user. Screenshots of exam-only scenarios are only served to staff and to
candidates whose open attempt contains that case study.

## Validation

`cargo test` validates the demo pack; a pack is validated in full before any write.
Typical errors: unknown module/scenario referenced by a question, quiz question without a
module or scenario, choice question without a correct answer, duplicate `ref`, missing
image, annotation out of bounds, invalid exam definition.
