# Generated operator examples

Generate examples using the actual incident, investigation and qualification renderers with synthetic fixtures:

```sh
cargo run --locked --example generate_gallery -- /absolute/empty/review-directory
```

The command writes `incident-example.md`, `investigation-example.md`, `qualification-example.md`, `recovery-example.md`, `structured-record-example.json` and a consistent `gallery.db` for viewing. It reuses the same public application services and sanitized fixtures as the tests, including a declared incident, proposed hypothesis, an unexecuted experiment, an unsent communication draft a fake blocked worker, a prospectively defined synthetic recovery window with independent attributed review, and a still-open corrective action. Response resolution does not repair the fixture or close the action. No external service/model is invoked. Imported source status is not recovery. The helper requires an empty directory and never commits the generated database.

To view the generated records in the current web application, run the normal server against that separate database with `--manual-workers`, then connect using its operator token. Real-clock deadlines from the fixed fixture may already be overdue; the supervisor will make them visible. The UI is the implemented operating interface, not a proposed mockup. Formal incident detail currently includes structured JSON panels; richer views remain on the roadmap.

Screenshots and rendered reports produced for operator review use only these invented records. No credential, account transcript, production telemetry or customer message belongs in an example export.
