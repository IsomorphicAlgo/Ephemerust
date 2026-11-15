# Orbit Mean-Elements Messages (OMM)

> **Implementation status.** Implemented in `src/omm.rs` (new in **1.2.0**): `Omm`,
> `OmmError`, `OmmFormat`, `select_omm`, and `Propagator::from_omm`. The `track` command
> accepts an OMM anywhere it accepts a TLE.

## Why the TLE needed a successor

A Two-Line Element set is a *fixed-column* format from the punched-card era: every field
lives at an exact character position, and the satellite catalog number gets five of them.
Five columns hold 99999 objects at most.

The public catalog used up its five-digit numbers on **2026-07-11**, when it reached 69999
(the block 70000–99999 is kept for temporary identifiers). Objects cataloged since then are
numbered **100000 and above**, and a plain TLE cannot name them.

Two answers exist:

| Answer | Reach | Idea |
|--------|-------|------|
| **Alpha-5** | 100000–339999 | Keep the TLE layout, but let the first catalog column be a letter worth two digits: `A` = 10 … `Z` = 33, skipping `I` and `O`. `A0000` is 100000; `Z9999` is 339999. |
| **OMM** | up to 999999999 | Drop fixed columns altogether. Every value is carried under a named keyword. |

Ephemerust reads both. `Tle::parse` decodes Alpha-5, and the `omm` module reads OMMs. Alpha-5
is a stopgap with a ceiling; the OMM is the format element-set providers publish for every
object, so it is the one to build on.

## The same physics in a different envelope

An OMM written for SGP4 holds the **same numbers** as a TLE: Kozai mean elements in the TEME
frame, with the same B* drag term. Nothing about the orbit model changes, and the same `sgp4`
engine propagates both. What changes is how the numbers are written down:

| Quantity | TLE | OMM keyword |
|----------|-----|-------------|
| Catalog number | line 1 and 2, columns 3–7 (5 digits, or Alpha-5) | `NORAD_CAT_ID` (up to 9 digits) |
| International designator | `98067A` (two-digit year) | `OBJECT_ID` = `1998-067A` |
| Epoch | `20194.88612269` (two-digit year, fractional day of year) | `EPOCH` = `2020-07-12T21:16:01.000416` |
| Eccentricity | `0001413` (decimal point implied) | `ECCENTRICITY` = `.0001413` |
| B* | `-31515-4` (implied point, packed exponent) | `BSTAR` = `-.31515E-4` |
| Mean motion | columns 53–63, rev/day | `MEAN_MOTION`, rev/day |
| Inclination, RAAN, argument of perigee, mean anomaly | fixed columns, degrees | `INCLINATION`, `RA_OF_ASC_NODE`, `ARG_OF_PERICENTER`, `MEAN_ANOMALY`, degrees |
| Integrity check | modulo-10 checksum per line | none (see below) |

Three TLE conventions that cause real mistakes are gone: the two-digit year (and its
1957–2056 window), the implied decimal point, and the packed exponent. One thing is lost: an
OMM has **no checksum**, so a corrupted value is caught only if it fails to parse or leaves
its physical range. That is why `Omm` validates ranges where `Tle` validates check digits.

## Four serializations, one message

The CCSDS standard (502.0-B-3) defines the keywords and two ways to write them, KVN and XML.
Providers add JSON and CSV, which reuse the same keywords.

**KVN** (key-value notation):

```text
CCSDS_OMM_VERS = 2.0
OBJECT_NAME = ISS (ZARYA)
OBJECT_ID = 1998-067A
CENTER_NAME = EARTH
REF_FRAME = TEME
TIME_SYSTEM = UTC
MEAN_ELEMENT_THEORY = SGP4
EPOCH = 2020-07-12T21:16:01.000416
MEAN_MOTION = 15.49507896
...
```

**XML**: one `<omm>` element per message, usually inside an `<ndm>` wrapper, with the same
keywords as element names grouped under `<metadata>`, `<meanElements>` and `<tleParameters>`.

**JSON**: an object per message (usually in an array), keyed by keyword. Some providers send
numbers (`"MEAN_MOTION": 15.49507896`), others strings (`"MEAN_MOTION": "15.49507896"`).

**CSV**: a header row of keywords, then one row per object.

JSON and CSV usually omit `CENTER_NAME`, `REF_FRAME`, `TIME_SYSTEM` and
`MEAN_ELEMENT_THEORY`, because for SGP4 data each has only one possible value.

### How Ephemerust reads them

Each serialization is reduced to the same thing, a list of `(keyword, value)` text pairs per
message, and a single function validates that list. A new serialization therefore needs only
a reader, and the four cannot drift apart in what they accept. `OmmFormat::detect` picks the
reader from the text, so callers do not name the format.

## What is checked, and why

| Check | Reason |
|-------|--------|
| `EPOCH`, `MEAN_MOTION`, `ECCENTRICITY`, `INCLINATION`, `RA_OF_ASC_NODE`, `ARG_OF_PERICENTER`, `MEAN_ANOMALY` and `BSTAR` must be present | SGP4 reads all eight. A missing `BSTAR` is an error, not a zero: assuming no drag would silently change the orbit. |
| `MEAN_MOTION_DOT`, `MEAN_MOTION_DDOT` default to 0 | SGP4 does not read them; they belong to the older SGP model. |
| 0 ≤ `ECCENTRICITY` < 1, 0° ≤ `INCLINATION` ≤ 180°, `MEAN_MOTION` > 0 | The range of closed orbits. This also catches TLE habits carried into an OMM: `0001413` is 1413, not 0.0001413. |
| `CENTER_NAME` = EARTH, `REF_FRAME` = TEME, `TIME_SYSTEM` = UTC, `MEAN_ELEMENT_THEORY` = SGP4 (when present) | Mean elements only mean something inside the theory that produced them. Elements fitted to another theory give wrong positions when run through SGP4. |
| `EPHEMERIS_TYPE` ≠ 4 | Type 4 marks SGP4-XP elements, which need a different propagator. |
| `NORAD_CAT_ID` is optional | An operator may publish elements before an object is cataloged. |

As with `TleError`, every `OmmError` states what was expected, what was found and why, and
`hint()` gives the next step.

## Using it

Library:

```rust
use chrono::Duration;
use ephemerust::omm::Omm;
use ephemerust::{ObserverLocation, Propagator};

let text = std::fs::read_to_string("group.json")?;       // a provider's JSON answer
let station = ObserverLocation { latitude: 47.9088, longitude: -122.2503, elevation: 0.0 };

for omm in Omm::parse_all(&text)? {
    let prop = Propagator::from_omm(&omm)?;
    let passes = prop.predict_passes(station, omm.epoch, omm.epoch + Duration::hours(24), 10.0)?;
    println!("{:?}: {} passes", omm.name, passes.len());
}
```

- `Omm::parse` reads exactly one message; `Omm::parse_all` reads all of them;
  `select_omm` picks one by name or catalog number (the OMM counterpart of `select_tle`).
- `Propagator::from_omm` builds the same reusable propagator a TLE does. Everything
  downstream (state, sub-point, look angles, passes, ground track, shadow) is a method on it.
- `Omm::from(&tle)` re-expresses a TLE as an OMM, so code written against `Omm` accepts
  both sources.

Command line: `track` detects the format from the content, so the existing flags work, and
`--omm-file`, `--omm`, `--omm-url` and `--name` are aliases that read better:

```bash
ephemerust track --omm-file group.json --name 100123 --predict-passes-hours 24
```

## Validation

`tests/omm_reference.rs` checks the whole path against python-sgp4, which wraps the reference
C++ SGP4 of Vallado et al. and is independent of the Rust `sgp4` crate. Both sides read the
same OMM files (`tests/data/omm/`): a CelesTrak sample in XML and CSV, and three synthetic
objects numbered 100123, 340000 and 799500123 covering a low orbit, a 12-hour Molniya orbit
and a geostationary orbit.

| Comparison | Worst case |
|------------|------------|
| TEME position vs. python-sgp4, 48 samples, −1 to +2 days from epoch | 0.024 mm |
| TEME velocity vs. python-sgp4, same samples | 0.008 µm/s |
| OMM vs. the TLE published for the same element set (CelesTrak sample) | no difference: the elements and the epoch are identical |
| Same, when the two epochs differ by rounding (0.864 ms steps in a TLE, microseconds in an OMM) | test bound 2 cm; a microsecond of epoch moves a low orbit by under 1 cm |

These numbers say that an OMM is read and handed to SGP4 correctly. They say nothing new
about how well SGP4 matches the real orbit; for that see
[accuracy-and-limits.md](accuracy-and-limits.md).

## Limits

- SGP4 mean elements only. OMMs for other theories are rejected, not approximated.
- The XML reader is a scanner for the flat OMM layout, not a general XML parser. It reads
  element text and the five predefined entities, and does not process DTDs or `CDATA`.
- A CSV field may be quoted but may not contain a line break.
- `Omm` is read-only: there is no OMM writer yet.
- Header keywords (`CREATION_DATE`, `ORIGINATOR`), covariance and user-defined parameters are
  ignored.

## References

- CCSDS 502.0-B-3, *Orbit Data Messages* (Blue Book), section 4: the OMM.
- CelesTrak, [GP data formats](https://celestrak.org/NORAD/documentation/gp-data-formats.php).
- D. Vallado, P. Crawford, R. Hujsak, T. S. Kelso, *Revisiting Spacetrack Report #3* (2006):
  the SGP4 reference implementation used for validation.
