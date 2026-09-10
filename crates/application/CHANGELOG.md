# Changelog

## [0.3.0](https://github.com/edpft/fitness-tracker/compare/v0.2.0...v0.3.0) (2026-09-10)


### ⚠ BREAKING CHANGES

* **peloton:** the entity is a cycling session, not a ride ([#103](https://github.com/edpft/fitness-tracker/issues/103))
* **plan:** a plan is the authored unit, and a programme holds its mesocycles ([#92](https://github.com/edpft/fitness-tracker/issues/92))
* **programme:** `fitness programme add` no longer takes a document path or `--into`, and writes no file. Authoring is the wizard.

### Features

* **cli:** performed against prescribed, paired by the id that links them ([#45](https://github.com/edpft/fitness-tracker/issues/45)) ([7b4703a](https://github.com/edpft/fitness-tracker/commit/7b4703ad864e9400dd48c5602e0df8246e92aaaf))
* **cycling:** FTP is derived from the tests that measured it ([#112](https://github.com/edpft/fitness-tracker/issues/112)) ([b3a550f](https://github.com/edpft/fitness-tracker/commit/b3a550f144090e20276473a897004e0894684c64))
* **cycling:** Peloton is read, and a cycling programme is authored and stored ([#74](https://github.com/edpft/fitness-tracker/issues/74)) ([52d2892](https://github.com/edpft/fitness-tracker/commit/52d28929921402b76cde4ad3ef0c049f5987f5bc))
* **delivery:** a destination's reply is kept ([#124](https://github.com/edpft/fitness-tracker/issues/124)) ([#125](https://github.com/edpft/fitness-tracker/issues/125)) ([c932adb](https://github.com/edpft/fitness-tracker/commit/c932adbc519869f4a0e64b7cc4c0f9030fe655d4))
* **peloton:** a Bike+ ride, composed from the workout record and its graph ([#102](https://github.com/edpft/fitness-tracker/issues/102)) ([fdb4505](https://github.com/edpft/fitness-tracker/commit/fdb4505c10a6d8662739ae17b55abec3fc39daf1))
* **peloton:** land the workout record, so cycling has a source ([#95](https://github.com/edpft/fitness-tracker/issues/95)) ([3a3613a](https://github.com/edpft/fitness-tracker/commit/3a3613a424da7075c076a0554e2cf1e582081eca))
* **peloton:** the entity is a cycling session, not a ride ([#103](https://github.com/edpft/fitness-tracker/issues/103)) ([1fbe402](https://github.com/edpft/fitness-tracker/commit/1fbe40280dc5721a9aeb336307ca80c650d760c0))
* **plan:** a plan is the authored unit, and a programme holds its mesocycles ([#92](https://github.com/edpft/fitness-tracker/issues/92)) ([cda3399](https://github.com/edpft/fitness-tracker/commit/cda339990c0c414917668973d10625d23ad251be))
* **prescribe:** derive every run, and identify a prescription by its shape ([#46](https://github.com/edpft/fitness-tracker/issues/46)) ([7e3355d](https://github.com/edpft/fitness-tracker/commit/7e3355ded6115f556502898bb5eaeae79514b591))
* the autumn block authors, and its sessions say what they mean ([#53](https://github.com/edpft/fitness-tracker/issues/53)) ([ac98571](https://github.com/edpft/fitness-tracker/commit/ac98571c5698764715ec3129f4f4b2f8bc38661d))
* the autumn block runs published programmes, gym and cycling ([#51](https://github.com/edpft/fitness-tracker/issues/51)) ([150b861](https://github.com/edpft/fitness-tracker/commit/150b8612014da1d9ebf2bb28bf11401cbba8a769))


### Bug Fixes

* **prescribe:** a published test week runs its own taper ([#120](https://github.com/edpft/fitness-tracker/issues/120)) ([#126](https://github.com/edpft/fitness-tracker/issues/126)) ([59d739c](https://github.com/edpft/fitness-tracker/commit/59d739c7a89bae4f9fb5de168db7312350c03ad7))
* **prescribe:** the record is what was lifted, and the heaviest set is the heaviest set ([#127](https://github.com/edpft/fitness-tracker/issues/127), [#129](https://github.com/edpft/fitness-tracker/issues/129)) ([#128](https://github.com/edpft/fitness-tracker/issues/128)) ([1d426d6](https://github.com/edpft/fitness-tracker/commit/1d426d64f6efdd2d5be34de5932f3f7bab6f5adc))
* **prescription:** a performance takes its role from its prescription ([#43](https://github.com/edpft/fitness-tracker/issues/43)) ([a7a407f](https://github.com/edpft/fitness-tracker/commit/a7a407f1ad85b5dea014fd4eac4d5bdddaa3a0f2))


### Code Refactoring

* **programme:** the questions author directly, and TOML goes ([#91](https://github.com/edpft/fitness-tracker/issues/91)) ([5439bb2](https://github.com/edpft/fitness-tracker/commit/5439bb2b44d529ec20c1606f571454ce4564c459))

## [0.2.0](https://github.com/edpft/fitness-tracker/compare/v0.1.0...v0.2.0) (2026-08-27)


### ⚠ BREAKING CHANGES

* **prescription:** a destination is a renderer that returns a receipt ([#18](https://github.com/edpft/fitness-tracker/issues/18))
* **prescription:** a test is a programme in its own right ([#17](https://github.com/edpft/fitness-tracker/issues/17))
* **prescription:** `programme` gains `name TEXT NOT NULL` with `UNIQUE (name, authored_at)`, so authoring the identical programme value twice is now refused. `ProgrammeStore::current` is replaced by `on(date)` and `windows()`; `ProgrammeAuthor::author` returns `Authored` beside the id; `Programme::new` and `rehydrate` take a name and a `Primary`; and `[programme] name` is a required document key.
* **prescription:** `programme_interruption` replaces `week` with `start_date` and `days`. `Calendar::new`, `Interruptions` and the interruption fixtures take `Skip` rather than `Date`, and `NotScheduled::Interrupted` and both `InvalidCalendar` interruption variants name a skip rather than a week.

### Features

* land Hevy workout history into raw ([#2](https://github.com/edpft/fitness-tracker/issues/2)) ([89fe28a](https://github.com/edpft/fitness-tracker/commit/89fe28a29e8b74e65935e523a2626011c0470ad8))
* **prescription:** a destination is a renderer that returns a receipt ([#18](https://github.com/edpft/fitness-tracker/issues/18)) ([27f7bdd](https://github.com/edpft/fitness-tracker/commit/27f7bddb7025517e01fb08e77cbeab5dfbeca295))
* **prescription:** a prescription is drafted, published, or performed ([#31](https://github.com/edpft/fitness-tracker/issues/31)) ([2d97d0c](https://github.com/edpft/fitness-tracker/commit/2d97d0cc66cfcf6be48f6937b12f4765bbc7d66e))
* **prescription:** a test is a programme in its own right ([#17](https://github.com/edpft/fitness-tracker/issues/17)) ([088f8bc](https://github.com/edpft/fitness-tracker/commit/088f8bc812dc19a3af6785abacb51dd8fd47d7dc))
* **prescription:** declared openings, per-implement scales, per-role back-offs ([#12](https://github.com/edpft/fitness-tracker/issues/12)) ([e6ca96a](https://github.com/edpft/fitness-tracker/commit/e6ca96a7f330049203f740108f2943ab2c10b8eb))
* **prescription:** programmes succeed one another, and linear never tests ([#16](https://github.com/edpft/fitness-tracker/issues/16)) ([299db4b](https://github.com/edpft/fitness-tracker/commit/299db4bc42f082125fe430459a9370b960923cf9))
* **prescription:** session skips, and what the test is an attempt at ([#14](https://github.com/edpft/fitness-tracker/issues/14)) ([216e8c7](https://github.com/edpft/fitness-tracker/commit/216e8c751c2203b47643edbfeb92ec9f95bec970))
* **schedule:** the operator's week, stored and shown ([#27](https://github.com/edpft/fitness-tracker/issues/27)) ([c0071ea](https://github.com/edpft/fitness-tracker/commit/c0071eaac014ab11ecc6acafb396ce7726a0ef95))


### Bug Fixes

* **prescription:** a stretch with two sides is held twice ([#26](https://github.com/edpft/fitness-tracker/issues/26)) ([c09483c](https://github.com/edpft/fitness-tracker/commit/c09483c385ef10db83e5e0d2293c10e69a8dbc38))
