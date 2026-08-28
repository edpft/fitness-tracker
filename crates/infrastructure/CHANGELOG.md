# Changelog

## [0.2.0](https://github.com/edpft/fitness-tracker/compare/v0.1.0...v0.2.0) (2026-08-27)


### ⚠ BREAKING CHANGES

* **cli:** `programme add` refuses when the schedule says nothing about the start date, where before it asked seven questions and authored against whatever was answered. Record the week first with `fitness schedule add`.
* **gym:** a pigeon stretch for external hip rotation ([#36](https://github.com/edpft/fitness-tracker/issues/36))
* **gym:** four exercises the autumn slots name, and an implement corrected ([#25](https://github.com/edpft/fitness-tracker/issues/25))
* **prescription:** a destination is a renderer that returns a receipt ([#18](https://github.com/edpft/fitness-tracker/issues/18))
* **prescription:** a test is a programme in its own right ([#17](https://github.com/edpft/fitness-tracker/issues/17))
* **prescription:** `programme` gains `name TEXT NOT NULL` with `UNIQUE (name, authored_at)`, so authoring the identical programme value twice is now refused. `ProgrammeStore::current` is replaced by `on(date)` and `windows()`; `ProgrammeAuthor::author` returns `Authored` beside the id; `Programme::new` and `rehydrate` take a name and a `Primary`; and `[programme] name` is a required document key.
* **prescription:** `programme_interruption` replaces `week` with `start_date` and `days`. `Calendar::new`, `Interruptions` and the interruption fixtures take `Skip` rather than `Date`, and `NotScheduled::Interrupted` and both `InvalidCalendar` interruption variants name a skip rather than a week.

### Features

* **cli:** `init` connects a source, without a key in the settings file ([#21](https://github.com/edpft/fitness-tracker/issues/21)) ([d3c515e](https://github.com/edpft/fitness-tracker/commit/d3c515efad5bf8b2f756e7d99ae4b969845576d7))
* **cli:** `init` prepares a machine and says what is still needed ([#20](https://github.com/edpft/fitness-tracker/issues/20)) ([6b6708d](https://github.com/edpft/fitness-tracker/commit/6b6708d2f605e7aa974484dfb58263661296df9a))
* **cli:** the programme wizard asks the seventeen slots ([#30](https://github.com/edpft/fitness-tracker/issues/30)) ([17a1530](https://github.com/edpft/fitness-tracker/commit/17a15307cd6d9072373de728b704a3efcae73538))
* **cli:** the store and the settings live where the specification says ([#19](https://github.com/edpft/fitness-tracker/issues/19)) ([9cd2ae1](https://github.com/edpft/fitness-tracker/commit/9cd2ae1fb01e8f9400e634f3a30996d73bdd8427))
* **cli:** the wizard asks dates and intents, and derives the plan ([#35](https://github.com/edpft/fitness-tracker/issues/35)) ([6e6f237](https://github.com/edpft/fitness-tracker/commit/6e6f23719411356b962ea1c192e2de13e53ced19))
* **cli:** the wizard authors a test and a ladder, not only a block ([#39](https://github.com/edpft/fitness-tracker/issues/39)) ([c518f47](https://github.com/edpft/fitness-tracker/commit/c518f474a9fba5a437ac847bd0f8124d5a85162f))
* **gym:** a pigeon stretch for external hip rotation ([#36](https://github.com/edpft/fitness-tracker/issues/36)) ([3cdb6e3](https://github.com/edpft/fitness-tracker/commit/3cdb6e32d59c4c978c966fb98a2d4f54e238b71a))
* **gym:** four exercises the autumn slots name, and an implement corrected ([#25](https://github.com/edpft/fitness-tracker/issues/25)) ([408984e](https://github.com/edpft/fitness-tracker/commit/408984e76607bbe519c44e51df2f34e0e294736b))
* land Hevy workout history into raw ([#2](https://github.com/edpft/fitness-tracker/issues/2)) ([89fe28a](https://github.com/edpft/fitness-tracker/commit/89fe28a29e8b74e65935e523a2626011c0470ad8))
* **prescription:** a destination is a renderer that returns a receipt ([#18](https://github.com/edpft/fitness-tracker/issues/18)) ([27f7bdd](https://github.com/edpft/fitness-tracker/commit/27f7bddb7025517e01fb08e77cbeab5dfbeca295))
* **prescription:** a prescription is drafted, published, or performed ([#31](https://github.com/edpft/fitness-tracker/issues/31)) ([2d97d0c](https://github.com/edpft/fitness-tracker/commit/2d97d0cc66cfcf6be48f6937b12f4765bbc7d66e))
* **prescription:** a programme reads the days it loses from the schedule ([#28](https://github.com/edpft/fitness-tracker/issues/28)) ([8295a3b](https://github.com/edpft/fitness-tracker/commit/8295a3b5fb4b2829fa23c3bffa839685b0ccd52a))
* **prescription:** a test is a programme in its own right ([#17](https://github.com/edpft/fitness-tracker/issues/17)) ([088f8bc](https://github.com/edpft/fitness-tracker/commit/088f8bc812dc19a3af6785abacb51dd8fd47d7dc))
* **prescription:** declared openings, per-implement scales, per-role back-offs ([#12](https://github.com/edpft/fitness-tracker/issues/12)) ([e6ca96a](https://github.com/edpft/fitness-tracker/commit/e6ca96a7f330049203f740108f2943ab2c10b8eb))
* **prescription:** programmes succeed one another, and linear never tests ([#16](https://github.com/edpft/fitness-tracker/issues/16)) ([299db4b](https://github.com/edpft/fitness-tracker/commit/299db4bc42f082125fe430459a9370b960923cf9))
* **prescription:** session skips, and what the test is an attempt at ([#14](https://github.com/edpft/fitness-tracker/issues/14)) ([216e8c7](https://github.com/edpft/fitness-tracker/commit/216e8c751c2203b47643edbfeb92ec9f95bec970))
* **schedule:** the operator's week, stored and shown ([#27](https://github.com/edpft/fitness-tracker/issues/27)) ([c0071ea](https://github.com/edpft/fitness-tracker/commit/c0071eaac014ab11ecc6acafb396ce7726a0ef95))


### Bug Fixes

* **prescription:** a stretch with two sides is held twice ([#26](https://github.com/edpft/fitness-tracker/issues/26)) ([c09483c](https://github.com/edpft/fitness-tracker/commit/c09483c385ef10db83e5e0d2293c10e69a8dbc38))
