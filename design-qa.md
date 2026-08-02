# Skills Navigation Design QA

## Scope

- Flow: leaderboard → skill detail → publisher → repository → skill detail
- State: dark theme, Simplified Chinese locale, loaded skills.sh data
- References:
  - `design-qa-skill-breadcrumb-reference.png`
  - `design-qa-skill-publisher-reference.png`
  - `design-qa-skill-repository-reference.png`
- Implementations:
  - `design-qa-skill-breadcrumb.jpg`
  - `design-qa-skill-publisher.jpg`
  - `design-qa-skill-repository.jpg`
- Combined comparisons:
  - `design-qa-skill-navigation-comparison.jpg`
  - `design-qa-skill-navigation-comparison-bottom.jpg`
- Browser viewport: `1280 x 720`

## Fidelity review

Each reference and its implementation were placed side by side in the same browser input before review.

- P0: none
- P1: none
- P2: none

The implementation matches the reference hierarchy and density: a four-level monospaced breadcrumb, publisher summary and source table, and repository summary, install command, mode tabs, and skill table. Intentional product-system differences are AgentSwitch's existing dark surface tokens, localized labels, and existing icon set.

## Interaction and runtime checks

- Every completed breadcrumb segment is a real button; the current segment is marked with `aria-current="page"`.
- `skills` returns to the leaderboard.
- The publisher segment opens the internal publisher page.
- The repository segment opens the internal repository page.
- Publisher source rows open the matching repository page.
- Repository skill rows open the matching in-app detail page.
- Command/prompt switching and command copying were exercised.
- The complete navigation flow was exercised in the in-app browser.
- No console errors remained after the duplicate breadcrumb key fix.

## Result

passed
