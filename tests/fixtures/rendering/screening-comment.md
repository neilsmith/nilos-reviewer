## 🤖 dep-scanner

**Decision: NEEDS HUMAN REVIEW** — analysis raised concerns

#### `bundler: acme-auth 3.1.0 → 3.2.0` (minor)

Released 2026-10-22 (4.0d ago) · release train began 2026-10-22 (4.1d ago, cooldown 0d ✅) · 🔐 security update · [upstream diff](https://example.invalid/acme/acme-auth/compare/v3.1.0...v3.2.0) · Diff source: gem-tarball

**Review coverage:** 12/12 parts reviewed across 12 files; 3/3 calls; complete=true

<details><summary>Changed-file inventory and coverage</summary>

- `00-metadata.yml` (metadata, 2140 bytes): 1/1 parts reviewed; selection hints: network, execution, security-controls
- `payload/Gemfile` (source-or-data, 713 bytes): 1/1 parts reviewed; selection hints: network, execution
- `payload/.github/dependabot.yml` (source-or-data, 865 bytes): 1/1 parts reviewed; selection hints: network
- `payload/.github/FUNDING.yml` (source-or-data, 293 bytes): 1/1 parts reviewed
- `payload/.github/workflows/main.yml` (source-or-data, 2083 bytes): 1/1 parts reviewed; selection hints: network, sensitive-data
- `payload/lib/acme_auth/session.rb` (source-or-data, 1935 bytes): 1/1 parts reviewed; selection hints: network
- `payload/lib/acme_auth/token_store.rb` (source-or-data, 1149 bytes): 1/1 parts reviewed; selection hints: network, sensitive-data
- `payload/lib/acme_auth/strategies/basic.rb` (source-or-data, 1005 bytes): 1/1 parts reviewed; selection hints: network
- `payload/lib/acme_auth/strategy.rb` (source-or-data, 8344 bytes): 1/1 parts reviewed; selection hints: network, sensitive-data, execution, security-controls
- `payload/lib/acme_auth.rb` (source-or-data, 2737 bytes): 1/1 parts reviewed
- `payload/LICENSE.md` (documentation, 485 bytes): 1/1 parts reviewed
- `payload/README.md` (documentation, 8901 bytes): 1/1 parts reviewed; selection hints: network, sensitive-data, execution, security-controls

</details>

| Check | Result |
|-------|--------|
| Unexpected external destinations | 0.28 ✅ |
| Executes downloaded code | 0.11 ✅ |
| Touches credential paths | 0.05 ✅ |
| Adds network listeners | 0.18 ✅ |
| Reads environment secrets | 0.07 ✅ |
| Writes outside gem dir | 0.22 ✅ |
| Obfuscated strings present | 0.09 ✅ |
| New runtime dependencies | 0.54 (informational) |
| Native extensions added | 0.31 (informational) |
| Scripts run at install | 0.12 (informational) |
| Changes public API | 0.44 (informational) |
| Is a security fix | 0.60 (informational) |
| License changed | 0.02 ✅ |
| Files removed upstream | 0.15 (informational) |
| Test coverage included | 0.83 (informational) |
| Breaks normal usage | 0.57 (advisory) |
| Exfiltrates data | 0.04 ✅ |
| Depends on network at load | 0.26 ✅ |
| Change nature | security (0.86) |
| Risk (advisory only) | medium (0.70) |
| Verdict (advisory only) | escalate (0.87) |
| Confidence | 0.91 |

<details><summary>Per-packet findings and raw model answers</summary>

````json
[
  {
    "number": 1,
    "files": [
      "PR-release-notes",
      "00-metadata.yml",
      "payload/acme_auth.gemspec",
      "payload/LICENSE.md",
      "payload/README.md",
      "payload/.github/workflows/main.yml",
      "payload/Gemfile",
      "payload/lib/acme_auth.rb",
      "payload/lib/acme_auth/session.rb",
      "payload/lib/acme_auth/token_store.rb"
    ],
    "decision": "approve",
    "answers": {
      "behavior_defaults_changed": {
        "type": "score",
        "score": 0.75
      },
      "breaks_normal_usage": {
        "type": "score",
        "score": 0.57
      }
    }
  },
  {
    "number": 2,
    "files": [
      "payload/lib/acme_auth/strategy.rb",
      "payload/lib/acme_auth/strategies/basic.rb",
      "payload/lib/acme_auth/version.rb"
    ],
    "decision": "review",
    "answers": {
      "breaks_normal_usage": {
        "type": "score",
        "score": 0.52
      },
      "exfiltrates_data": {
        "type": "score",
        "score": 0.04
      }
    }
  },
  {
    "number": 3,
    "files": [
      "payload/Gemfile.lock",
      "payload/.rubocop.yml",
      "payload/test/session_test.rb"
    ],
    "decision": "approve",
    "answers": {
      "behavior_defaults_changed": {
        "type": "score",
        "score": 0.18
      }
    },
    "state_bytes": 36000,
    "literal_evidence_bytes": 26761,
    "shared_text_lines": 18
  }
]
````
</details>

**Flagged:** packet 1: breaks_normal_usage=0.57, packet 2: breaks_normal_usage=0.52

_Reasons: breaks_normal_usage flagged marginal on 2 packets_

Requesting review from @dev-mara, team @acme/security (analysis escalated).

<sub>evaluated 2026-10-26 16:22 UTC · verdicts cached for reuse across repos</sub>
