# Power Zone Build, read from the Peloton API

**Transcribed 2026-09-05, and not from screenshots.** Every duration below is
the API's own, via `api.onepeloton.com`. The skeleton — which class is which
microcycle and session — is the operator's, because the API does not serve
programme structure (see decision 0032).

**Build is five microcycles of three sessions.** That is what the programme is.
What it answers when asked for a smaller shape is a separate question, and the
answer is at the bottom of this file.

## The classes

| µcycle | session | duration | class | instructor | warm up | ride | cool | zones in the ride |
|---|---|---|---|---|---|---|---|---|
| 1 | 1 | 45 min | 45 min Power Zone Endurance Ride | Matt Wilpers | 13:00 | 31:00 | 1:00 | Z1 1:00 · Z2 12:00 · Z3 18:00 |
| 1 | 2 | 45 min | 45 min Power Zone Endurance Ride | Olivia Amato | 12:05 | 31:55 | 1:00 | Z1 1:00 · Z2 9:00 · Z3 21:55 |
| 1 | 3 | 60 min | 60 min Power Zone Endurance Ride | Christine D'Ercole | 10:56 | 48:00 | 1:04 | Z1 1:00 · Z2 18:00 · Z3 29:00 |
| 2 | 1 | 45 min | 45 min Power Zone Ride | Denis Morton | 12:00 | 32:00 | 1:00 | Z1 5:00 · Z3 13:00 · Z4 14:00 |
| 2 | 2 | 45 min | 45 min Power Zone Endurance Ride | Ben Alldis | 11:01 | 33:00 | 0:59 | Z1 1:00 · Z2 9:00 · Z3 23:00 |
| 2 | 3 | 60 min | 60 min Power Zone Endurance Ride | Olivia Amato | 8:00 | 51:00 | 1:00 | Z1 3:00 · Z2 11:30 · Z3 36:30 |
| 3 | 1 | 45 min | 45 min Power Zone Ride | Ben Alldis | 12:00 | 32:00 | 1:00 | Z1 7:00 · Z2 4:00 · Z4 10:00 · Z5 11:00 |
| 3 | 2 | 45 min | 45 min Power Zone Ride | Christine D'Ercole | 13:00 | 31:00 | 1:00 | Z1 5:00 · Z3 15:00 · Z4 11:00 |
| 3 | 3 | 60 min | 60 min Power Zone Endurance | Denis Morton | 13:00 | 46:00 | 1:00 | Z1 1:00 · Z2 10:00 · Z3 35:00 |
| 4 | 1 | 45 min | 45 min Power Zone Max Ride | Olivia Amato | 13:00 | 31:00 | 1:00 | Z1 14:00 · Z4 3:45 · Z5 8:00 · Z6 5:00 · Z7 0:15 |
| 4 | 2 | 45 min | 45 min Power Zone Ride | Matt Wilpers | 13:00 | 31:00 | 1:00 | Z1 4:00 · Z3 16:00 · Z4 8:00 · Z5 3:00 |
| 4 | 3 | 60 min | 60 min Power Zone Ride | Ben Alldis | 11:01 | 47:59 | 1:00 | Z1 7:00 · Z3 22:59 · Z4 18:00 |
| 5 | 1 | 45 min | 45 min Power Zone Endurance Ride | Christine D'Ercole | 11:05 | 32:55 | 1:00 | Z1 1:00 · Z2 8:45 · Z3 23:10 |
| 5 | 2 | 45 min | 45 min Power Zone Endurance Ride | Denis Morton | 13:00 | 31:00 | 1:00 | Z1 1:00 · Z2 12:00 · Z3 18:00 |
| 5 | 3 | 10 min | 10 min FTP Warm Up Ride | Matt Wilpers | 10:00 | 0:00 | 0:00 | — |
| 5 | 3 | 20 min | 20 min FTP Test Ride | Matt Wilpers | 0:00 | 20:00 | 0:00 | **no zones — this is the test** |

**Every riding class tiles exactly**: the zone plan accounts for 100% of the
ride segment in all fourteen. The one that does not is the FTP test, which
carries no zone plan at all — which is decision 0025's `Ride::Effort`,
corroborated by the source rather than by us.

**The cool-down column is the class's own.** The operator rides a separate
`5 min Cool Down Ride` after the main class; it is 116 rides in his record and
122 of his 153 riding days carry more than one class. A session is one or more
classes, and this table does not show the second one.

## The classes, by identifier

**The `classId` is the whole of what names a class** — not the title, not the
instructor, both of which repeat within this programme. Supplied by the operator
on 2026-09-05 and read from the API the same day; every row below reproduced the
table above exactly.

The `code` parameter of the share link the app produces is **deliberately
absent**, as it is for Peak: it decodes to two further identifiers, one of them
plausibly the operator's own Peloton user id, so it is not repository content.

| µ | s | class id |
|---|---|---|
| 1 | 1 | `9f8f3af689cc4f0db9afa013d4676ed6` |
| 1 | 2 | `44867a5486184a09b8ca135a6d8c7494` |
| 1 | 3 | `e825140788a84d31b3948419e50bedb5` |
| 2 | 1 | `7fa9796be8484c9987122d357de25fc7` |
| 2 | 2 | `49c2c7626aba4effa39b53b85c0e16f6` |
| 2 | 3 | `d3d85447e9d14ea29344a002823a82ca` |
| 3 | 1 | `a5f95a660f5b4a84ac6a86aa4468ea1d` |
| 3 | 2 | `daa6ce2d3a454d1c82937fa926a59db4` |
| 3 | 3 | `414a518108ea4c5cada00ab9899a9d8d` |
| 4 | 1 | `4c2110dd4de74c9b9a4a4e9176501702` |
| 4 | 2 | `47ad3764fbfb4774967958874465bee4` |
| 4 | 3 | `1b768ee376c546ae92493d8301eeec85` |
| 5 | 1 | `725d618516674f7581d4d566fe3f0655` |
| 5 | 2 | `4355cbf8734648c1a26c2f6d354035c5` |
| 5 | 3 | `1eabf70b20744f48b99259f93889ced5` — 10 min FTP Warm Up Ride |
| 5 | 3 | `4d302bef49574118a071269bed38bd30` — 20 min FTP Test Ride |

**Sixteen classes, fifteen sessions.** The last is two, because the test class
carries no warm-up of its own (decision 0033).

## The intervals, in order

**µ1 s1 — 45 min Power Zone Endurance Ride, Matt Wilpers**  
`Z1 1:00  Z3 3:00  Z2 3:00  Z3 4:00  Z2 3:00  Z3 5:00  Z2 3:00  Z3 4:00  Z2 3:00  Z3 2:00`

**µ1 s2 — 45 min Power Zone Endurance Ride, Olivia Amato**  
`Z1 1:00  Z3 2:55  Z2 2:00  Z3 5:00  Z2 2:00  Z3 7:00  Z2 3:00  Z3 5:00  Z2 2:00  Z3 2:00`

**µ1 s3 — 60 min Power Zone Endurance Ride, Christine D'Ercole**  
`Z1 1:00  Z3 4:00  Z2 3:00  Z3 4:00  Z2 3:00  Z3 5:00  Z2 4:00  Z3 5:00  Z2 4:00  Z3 6:00  Z2 4:00  Z3 5:00`

**µ2 s1 — 45 min Power Zone Ride, Denis Morton**  
`Z1 1:00  Z4 2:00  Z3 2:00  Z4 2:00  Z3 2:00  Z1 2:00  Z4 3:00  Z3 3:00  Z4 3:00  Z3 3:00  Z1 2:00  Z4 2:00  Z3 2:00  Z4 2:00  Z3 1:00`

**µ2 s2 — 45 min Power Zone Endurance Ride, Ben Alldis**  
`Z1 1:00  Z3 6:00  Z2 3:00  Z3 6:00  Z2 3:00  Z3 6:00  Z2 3:00  Z3 5:00`

**µ2 s3 — 60 min Power Zone Endurance Ride, Olivia Amato**  
`Z1 1:00  Z2 1:30  Z3 1:30  Z1 2:00  Z3 4:00  Z2 2:00  Z3 6:00  Z2 2:00  Z3 8:00  Z2 2:00  Z3 8:00  Z2 2:00  Z3 6:00  Z2 2:00  Z3 3:00`

**µ3 s1 — 45 min Power Zone Ride, Ben Alldis**  
`Z1 1:00  Z4 5:00  Z1 2:00  Z5 2:00  Z2 1:00  Z5 2:00  Z2 1:00  Z5 2:00  Z1 2:00  Z4 5:00  Z1 2:00  Z5 2:00  Z2 1:00  Z5 2:00  Z2 1:00  Z5 1:00`

**µ3 s2 — 45 min Power Zone Ride, Christine D'Ercole**  
`Z1 1:00  Z3 6:00  Z4 5:00  Z1 2:00  Z3 5:00  Z4 4:00  Z1 2:00  Z3 4:00  Z4 2:00`

**µ3 s3 — 60 min Power Zone Endurance, Denis Morton**  
`Z1 1:00  Z3 8:00  Z2 2:00  Z3 6:00  Z2 2:00  Z3 4:00  Z2 2:00  Z3 8:00  Z2 2:00  Z3 6:00  Z2 2:00  Z3 3:00`

**µ4 s1 — 45 min Power Zone Max Ride, Olivia Amato**  
`Z1 1:00  Z5 2:00  Z1 1:00  Z6 1:00  Z1 1:00  Z5 2:00  Z1 1:00  Z6 1:00  Z1 3:00  Z5 2:00  Z1 1:00  Z6 1:00  Z1 1:00  Z5 2:00  Z1 1:00  Z6 1:00  Z1 3:00  Z6 0:30  Z4 2:15  Z7 0:15  Z1 1:00  Z6 0:30  Z4 1:30`

**µ4 s2 — 45 min Power Zone Ride, Matt Wilpers**  
`Z1 1:00  Z3 2:00  Z4 2:00  Z3 2:00  Z5 1:00  Z3 2:00  Z4 2:00  Z3 2:00  Z5 1:00  Z1 3:00  Z3 2:00  Z4 2:00  Z3 2:00  Z5 1:00  Z3 2:00  Z4 2:00  Z3 2:00`

**µ4 s3 — 60 min Power Zone Ride, Ben Alldis**  
`Z1 1:00  Z4 3:00  Z3 4:00  Z4 3:00  Z1 2:00  Z3 4:00  Z4 3:00  Z3 4:00  Z1 2:00  Z4 3:00  Z3 4:00  Z4 3:00  Z1 2:00  Z3 4:00  Z4 3:00  Z3 2:59`

**µ5 s1 — 45 min Power Zone Endurance Ride, Christine D'Ercole**  
`Z1 1:00  Z3 5:00  Z2 3:00  Z3 7:15  Z2 2:45  Z3 7:00  Z2 3:00  Z3 3:55`

**µ5 s2 — 45 min Power Zone Endurance Ride, Denis Morton**  
`Z1 1:00  Z3 3:00  Z2 3:00  Z3 4:00  Z2 3:00  Z3 5:00  Z2 3:00  Z3 4:00  Z2 3:00  Z3 2:00`

**µ5 s3 — 10 min FTP Warm Up Ride, Matt Wilpers**  
_Warm-up only; no ride._

**µ5 s3 — 20 min FTP Test Ride, Matt Wilpers**  
_20 minutes, as hard as can be held. No zones._

## The shape, and what it answers when asked for four by two

Hard work (Z4 and above) as a share of that microcycle's riding:

| µcycle | all 3 sessions | sessions 1+3 |
|---|---|---|
| 1 | 0% | 0% |
| 2 | 12% | 17% |
| 3 | 29% | 27% |
| 4 | 42% | 44% |
| 5 | 0% | 0% |

**The 3:1 structure is in microcycles 2 to 5** — 12% → 29% → 42% → 0%, three
working microcycles and a deload. Microcycles 1 to 4 run 0% → 12% → 29% → 42%
and peak last, which is not a mesocycle shape.

So **asked for four microcycles of two sessions, Build answers sessions 1 and 3
of microcycles 2 to 5.** Sessions 1+3 diverge 8.5 percentage points from the
whole programme's zone profile, against 17.5 for 1+2 and 24.9 for 2+3, and
their weekly arc tracks it — 17% → 27% → 44% → 0%. Microcycle 1 is what the
programme sheds when asked for four; it is not a lead-in outside the mesocycle.
