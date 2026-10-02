# Personal data (GDPR)

## Data collected

Only what training requires:

| Data | Purpose |
| --- | --- |
| E-mail, name | Account, sign-in, certificates |
| Organization and role in it | Access to tracks, requirement tracking |
| Progress, quiz scores, exam answers and results | Training follow-up, certification |
| Certifications | Badges, verification, requirement levels |
| Ratings of tracks | Content quality |
| Sign-in and administration events | Security (audit log) |

No tracking, analytics or third-party scripts; fonts, images and videos are served by the
instance or by storage it controls.

## Legal basis and information

Define it in the instance's privacy policy (`instance.privacy_policy_url`, linked in the
footer, e-mails and sign-in page): typically the performance of the partner contract for
partners, the customer contract for customer administrators, and consent for public
learners.

## Retention

The platform keeps data until the account is deleted. Housekeeping deletes expired
sessions and sign-in links, and sent e-mails after 30 days. Set retention periods in the
privacy policy (e.g. accounts inactive for 3 years) and apply them; inactive accounts can
be listed from `users.last_login_at`.

## Learners' rights (self-service, *My profile*)

- **Access / portability**: *Download my data* exports everything as JSON.
- **Rectification**: name editable; e-mail through the identity provider or support.
- **Erasure**: *Delete my account* deletes the account, memberships, progress, attempts
  and certifications (their verification pages then answer "not found"). Anonymous
  per-question statistics are kept; audit entries lose the link to the person.
- **Public page**: the verification page shows only the name, the track and the dates; it
  can be made private at any time (it then answers "not found", and the Open Badges
  assertion too). Open Badges assertions publish a salted hash of the e-mail, never the
  e-mail itself.

## Screenshots

Scenario screenshots are made on a demo tenant with fictitious data only: no real server
name, address or user of a customer.
