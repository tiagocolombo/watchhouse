# Generated operator examples

Generate examples using the actual incident, investigation and qualification renderers with synthetic fixtures:

```sh
cargo run --locked --example generate_gallery -- /absolute/empty/review-directory
```

The command writes `incident-example.md`, `investigation-example.md`, `qualification-example.md`, `recovery-example.md`, `postmortem-example.md`, `structured-record-example.json` and a consistent `gallery.db` for viewing. It reuses the same public application services and sanitized fixtures as the tests, including a declared incident, proposed hypothesis, an unexecuted experiment, an unsent communication draft a fake blocked worker followed by acknowledged specialist handoff/completion, a prospectively defined synthetic recovery window with independent attributed review, and a still-open corrective action. Response resolution does not repair the fixture or close the action. The postmortem keeps cause/total impact unknown and references the open corrective action; an independently reviewed scoped lesson is frozen into the successor fake briefing. No external service/model is invoked. Imported source status is not recovery. The helper requires an empty directory and never commits the generated database.

To view the generated records in the current web application, run the normal server against that separate database with `--manual-workers`, then connect using its operator token. The active recovery/knowledge scenario uses a near-current manual clock and a past reported window. Original source/declaration history remains a fixed synthetic fixture; overdue duties are shown by the supervisor. Older generated databases can correctly show expired lessons and overdue actions. The UI is the implemented operating interface, not a proposed mockup. Formal incident detail currently includes structured JSON panels; richer views remain on the roadmap.

Screenshots and rendered reports produced for operator review use only these invented records. No credential, account transcript, production telemetry or customer message belongs in an example export.

CI packages this scenario and its reports with the native coordinator for each tested OS/architecture. [TESTING.md](../TESTING.md) describes the test-build workflow. No SDK is needed to run a downloaded bundle.
