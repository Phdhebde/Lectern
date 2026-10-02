# Exams, certifications and requirements

## Learner path

1. **Sign in** (OIDC or e-mail link), then optionally request to join an organization
   with its join code. The training manager approves the request. Without organization,
   the learner is an *individual* and only sees `public` tracks.
2. **Modules**: video (position saved, resumed), recap sheet, attachments, quiz with
   correction and explanations.
3. **Scenarios** (mandatory when `scenarios_required`): context, steps with annotated
   screenshots and expected results, pitfalls, verification questions.
4. **Exam**, unlocked when all modules (and scenarios) are completed and the prerequisite
   certification is valid.
5. **Certification**: badge, PDF certificate, public verification page, Open Badges
   assertion, LinkedIn "add to profile" link. Expiry alerts, then recertification.

## Exam integrity

- Questions drawn at random from the bank; the bank must contain at least `bank_factor`
  (default 3) times the number of questions drawn, otherwise the exam is not offered.
- Answer choices shuffled per attempt; checkboxes always (the number of correct answers is
  not revealed).
- Case studies: exam-only scenarios, never shown in the track, drawn at random and
  different from the learner's previous attempt; at least twice the number drawn must exist.
- Timed sections, deadline enforced by the server (30 s network grace); an abandoned
  section is graded automatically when its time is up.
- One exam in progress per account (database constraint).
- Exam answers and explanations are never sent to learners; exam-only screenshots are
  only served to candidates whose open attempt contains them, and to staff.
- Grading is exact match per question; scores are floored (74.9 % never passes 75 %).

## Attempts policy

`free_attempts` attempts are included (unlimited when omitted). Beyond them, every new
attempt needs an **attempt credit** and respects `cooldown_days` after the previous
attempt. Credits are granted by administrators today (*Administration › Users*) and will
be created by the payment module later. Attempts are counted since the last success.

## Validity and recertification

- `validity_months` per track; tracks without it never expire.
- Recertification opens `alerts.recert_window_days` before expiry, with the short
  `recert_exam` (or the full exam if none, or if the current certification is
  provisional). A renewal extends from the previous expiry date, so renewing early costs
  nothing.
- **New major product version:** *Administration › Settings › New major version*. Every
  valid certification obtained on another version then expires at most
  `alerts.major_version_grace_days` later. Also update `instance.product_major_version`
  so new certifications record the new version.
- Alerts at 90, 30 and 7 days (configurable) to the learner and their training managers;
  each threshold is sent once, and a late first alert does not trigger the older ones.
- Expired, revoked or superseded certifications no longer count for requirements.

## Manual evaluation

Written answers (e.g. complex case studies) put the attempt in *pending review*.
Trainers see it in *Reviews*: the scenario, the answers and the expected choices, an
evaluation grid, a comment sent to the learner, and the decision. An evaluator cannot
review their own attempt. Interviews happen outside the platform; record them as a grid
criterion.

## Requirement levels (partner tiers)

Levels are generic: for an organization kind (`partner` or `customer`), a name, a rank and
the number of valid certified members required per track. They are defined in the content
pack or in *Administration › Requirements*. Organizations are assigned a current level.

- Training managers see, for each level, valid certified members vs required and the gap.
- Channel managers see every organization, its level, whether it is met, the highest level
  met, and can export a CSV.
- The partner portal reads `GET /api/v1/certified` with an API token.
