//! The table and the chart of the section "Time" of `docs/performance.md`, written from the
//! record of the runs of the benchmark `checks`.
//!
//! The record is the output of each run, as `cargo bench -p puns --features unstable --bench
//! checks` prints it, after a line `== run N`. The table gives the median of the runs for each
//! case, and the chart the ratio of the two medians.

#![expect(
  clippy::suboptimal_flops,
  reason = "positions in pixels, which `mul_add` makes harder to read"
)]

use std::{fmt::Write as _, fs, path::Path};

use anyhow::{Context, Result, bail, ensure};

/// The record that the table and the chart are written from.
const RECORD: &str = "docs/checks-2026-11-21.txt";
/// The file that holds the table, between the two markers.
const PERFORMANCE: &str = "docs/performance.md";
const START: &str = "<!-- checks:start -->";
const END: &str = "<!-- checks:end -->";
/// The chart.
const CHART: &str = "docs/charts/checks.svg";

/// One case of the benchmark: its name, and the time of each run by `puns` and by hand, in
/// nanoseconds per check.
struct Case {
  name: String,
  puns: Vec<f64>,
  hand: Vec<f64>,
}

impl Case {
  fn puns(&self) -> f64 {
    median(&self.puns)
  }

  fn hand(&self) -> f64 {
    median(&self.hand)
  }

  /// The time of `puns` over the time by hand: below 1, `puns` is faster.
  fn ratio(&self) -> f64 {
    self.puns() / self.hand()
  }

  /// The group of the case, which the chart separates from the next group by a gap.
  fn group(&self) -> &str {
    if self.name.starts_with('[') {
      "slices"
    } else if self.name.starts_with("crc") {
      "crc"
    } else {
      self.name.split(',').next().unwrap_or("")
    }
  }
}

/// Writes the table and the chart, or with `check`, fails if either differs from what it writes.
pub fn run(root: &Path, check: bool) -> Result<()> {
  let record =
    fs::read_to_string(root.join(RECORD)).with_context(|| format!("reading {RECORD}"))?;
  let (runs, cases) = parse(&record)?;
  let performance = fs::read_to_string(root.join(PERFORMANCE))?;
  let (Some(start), Some(end)) = (performance.find(START), performance.find(END)) else {
    bail!("{PERFORMANCE} has no `{START}` and `{END}`");
  };
  let table = format!(
    "{START}\n{}\n\n<details>\n<summary>The times in nanoseconds per check, the median of {runs} \
     runs</summary>\n\n{}\n</details>\n{END}",
    image(&cases),
    table(&cases),
  );
  let performance = format!("{}{table}{}", &performance[..start], &performance[end + END.len()..]);
  let chart = chart(&cases, runs);
  for (path, text) in [(PERFORMANCE, &performance), (CHART, &chart)] {
    let path_buf = root.join(path);
    if check {
      let old = fs::read_to_string(&path_buf).unwrap_or_default();
      ensure!(old == *text, "{path} is not what `cargo xtask checks-chart` writes from {RECORD}");
    } else {
      fs::create_dir_all(path_buf.parent().context("a path without a parent")?)?;
      fs::write(&path_buf, text)?;
      println!("wrote {path}");
    }
  }
  Ok(())
}

/// Returns the number of runs of `record`, and its cases in the order of the benchmark.
fn parse(record: &str) -> Result<(usize, Vec<Case>)> {
  let (mut runs, mut cases, mut rows) = (0, Vec::<Case>::new(), false);
  for line in record.lines() {
    if line.starts_with("== run") {
      (runs, rows) = (runs + 1, false);
    } else if line.starts_with("ns per check") {
      rows = true;
    } else if rows && !line.trim().is_empty() {
      // The name, then the time by `puns`, the time by hand and their ratio.
      let (rest, _) = last_field(line).with_context(|| format!("a row without times: {line}"))?;
      let (rest, hand) =
        last_field(rest).with_context(|| format!("a row without times: {line}"))?;
      let (name, puns) =
        last_field(rest).with_context(|| format!("a row without times: {line}"))?;
      let (name, puns, hand): (String, f64, f64) = (name.to_owned(), puns.parse()?, hand.parse()?);
      if let Some(case) = cases.iter_mut().find(|case| case.name == name) {
        case.puns.push(puns);
        case.hand.push(hand);
      } else if runs == 1 {
        cases.push(Case { name, puns: vec![puns], hand: vec![hand] });
      } else {
        bail!("run {runs} has a case that the first run has not: {name}");
      }
    }
  }
  ensure!(runs > 0 && !cases.is_empty(), "{RECORD} has no run");
  for case in &cases {
    ensure!(case.puns.len() == runs, "the case `{}` is not in every run", case.name);
  }
  Ok((runs, cases))
}

/// Returns `line` without its last field, and that field, or `None` if it has one field or none.
fn last_field(line: &str) -> Option<(&str, &str)> {
  let line = line.trim_end();
  let at = line.rfind(char::is_whitespace)?;
  Some((line[..at].trim_end(), &line[at + 1..]))
}

fn median(values: &[f64]) -> f64 {
  let mut sorted = values.to_vec();
  sorted.sort_by(f64::total_cmp);
  let middle = sorted.len() / 2;
  if sorted.len() % 2 == 1 {
    sorted[middle]
  } else {
    f64::midpoint(sorted[middle - 1], sorted[middle])
  }
}

/// Returns `time` as the table writes it: two decimals below 10, one below 100, and a whole
/// number with thousands separators from 100.
fn time(time: f64) -> String {
  if time < 10.0 {
    format!("{time:.2}")
  } else if time < 100.0 {
    format!("{time:.1}")
  } else {
    let digits = format!("{time:.0}");
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
      if i > 0 && (digits.len() - i) % 3 == 0 {
        out.push(',');
      }
      out.push(digit);
    }
    out
  }
}

/// Returns `ratio` as the chart writes it: two significant digits below 0.1, two decimals above.
fn ratio(ratio: f64) -> String {
  format!("{}×", ratio_number(ratio))
}

/// Returns `ratio` as [`ratio`] writes it, without the sign.
fn ratio_number(ratio: f64) -> String {
  if ratio < 0.1 { format!("{ratio:.3}") } else { format!("{ratio:.2}") }
}

/// Returns the Markdown image of the chart, whose text says what it shows.
///
/// The text is on two lines, since the lines of the document are at most 100 columns.
fn image(cases: &[Case]) -> String {
  let (lo, hi) = range(cases);
  let faster = cases.iter().filter(|case| case.ratio() < 1.0).count();
  format!(
    "![The time of each check by puns relative to the same check by hand: from {} to {} times,\n\
     below 1 for {faster} of the {} cases](charts/checks.svg)",
    ratio_number(lo),
    ratio_number(hi),
    cases.len(),
  )
}

/// Returns the least and the greatest ratio of `cases`.
fn range(cases: &[Case]) -> (f64, f64) {
  cases
    .iter()
    .map(Case::ratio)
    .fold((f64::MAX, f64::MIN), |(lo, hi), ratio| (lo.min(ratio), hi.max(ratio)))
}

fn table(cases: &[Case]) -> String {
  let mut out = String::from("| Case | `puns` | by hand |\n|---|---:|---:|\n");
  for case in cases {
    let _ = writeln!(out, "| `{}` | {} | {} |", case.name, time(case.puns()), time(case.hand()));
  }
  out
}

/// The width of the chart, and where its parts are, in pixels.
const WIDTH: f64 = 760.0;
const LABELS: f64 = 352.0;
const PLOT: (f64, f64) = (364.0, 690.0);
const TOP: f64 = 74.0;
const ROW: f64 = 18.0;
const GAP: f64 = 8.0;
const BAR: f64 = 11.0;
/// The ratios at the two ends of the axis, which is logarithmic.
const DOMAIN: (f64, f64) = (0.025, 2.0);
const TICKS: [f64; 7] = [0.025, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0];

/// Returns the horizontal position of `ratio`.
fn x(ratio: f64) -> f64 {
  let (lo, hi) = (DOMAIN.0.ln(), DOMAIN.1.ln());
  PLOT.0 + (ratio.clamp(DOMAIN.0, DOMAIN.1).ln() - lo) / (hi - lo) * (PLOT.1 - PLOT.0)
}

/// Returns a bar from `from` to `to` whose top is at `top`, rounded at `to`, its data end, and
/// square at `from`, the baseline.
fn bar(from: f64, to: f64, top: f64) -> String {
  let radius = 4.0_f64.min((to - from).abs() / 2.0);
  let sign = if to < from { -1.0 } else { 1.0 };
  let straight = to - sign * radius;
  format!(
    "M{from:.1},{top:.1}H{straight:.1}A{radius},{radius} 0 0 {sweep} {to:.1},{start:.1}V{end:.1}\
     A{radius},{radius} 0 0 {sweep} {straight:.1},{bottom:.1}H{from:.1}Z",
    sweep = u8::from(sign > 0.0),
    start = top + radius,
    end = top + BAR - radius,
    bottom = top + BAR,
  )
}

fn escape(text: &str) -> String {
  text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn chart(cases: &[Case], runs: usize) -> String {
  let groups = cases.windows(2).filter(|pair| pair[0].group() != pair[1].group()).count();
  #[expect(clippy::cast_precision_loss, reason = "a few dozen cases")]
  let bottom = TOP + cases.len() as f64 * ROW + groups as f64 * GAP;
  let height = bottom + 44.0;
  let (lo, hi) = range(cases);
  let mut out = String::new();
  let _ = write!(
    out,
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="title desc">
<title id="title">Time of each check by puns, relative to the same check written by hand</title>
<desc id="desc">The median of {runs} runs of each of {n} cases. The ratio goes from {lo} to {hi}: below 1, puns is faster.</desc>
<style>
svg {{ --surface: #fcfcfb; --primary: #0b0b0b; --secondary: #52514e; --muted: #898781; --grid: #e1e0d9; --faster: #2a78d6; --slower: #e34948; }}
@media (prefers-color-scheme: dark) {{
  svg {{ --surface: #1a1a19; --primary: #f0efec; --secondary: #c3c2b7; --muted: #898781; --grid: #2c2c2a; --faster: #3987e5; --slower: #e66767; }}
}}
text {{ font-family: system-ui, -apple-system, "Segoe UI", sans-serif; font-size: 12px; fill: var(--secondary); }}
.title {{ font-size: 15px; font-weight: 600; fill: var(--primary); }}
.name {{ fill: var(--primary); }}
.tick {{ fill: var(--muted); font-variant-numeric: tabular-nums; }}
.value {{ font-variant-numeric: tabular-nums; }}
.grid {{ stroke: var(--grid); stroke-width: 1; }}
.axis {{ stroke: var(--muted); stroke-width: 1; }}
.faster {{ fill: var(--faster); }}
.slower {{ fill: var(--slower); }}
</style>
<rect width="{WIDTH}" height="{height}" rx="8" fill="var(--surface)"/>
<text class="title" x="16" y="28">Time of each check by puns, relative to the same check by hand</text>
<text x="16" y="48">The median of {runs} runs on an Apple M4 Max, rustc 1.100.0. Below 1×, puns is faster.</text>
"#,
    n = cases.len(),
    lo = ratio(lo),
    hi = ratio(hi),
  );
  for tick in TICKS {
    let class = if (tick - 1.0).abs() < f64::EPSILON { "axis" } else { "grid" };
    let _ = writeln!(
      out,
      r#"<line class="{class}" x1="{x:.1}" x2="{x:.1}" y1="{top:.1}" y2="{bottom:.1}"/><text class="tick" x="{x:.1}" y="{label:.1}" text-anchor="middle">{tick}×</text>"#,
      x = x(tick),
      top = TOP - 6.0,
      label = bottom + 16.0,
    );
  }
  let _ = writeln!(
    out,
    r#"<text x="{left:.1}" y="{y:.1}" text-anchor="end">← puns faster</text><text x="{right:.1}" y="{y:.1}">by hand faster →</text>"#,
    left = x(1.0) - 6.0,
    right = x(1.0) + 6.0,
    y = bottom + 34.0,
  );
  let mut y = TOP;
  for (i, case) in cases.iter().enumerate() {
    if i > 0 && cases[i - 1].group() != case.group() {
      y += GAP;
    }
    let (r, middle) = (case.ratio(), y + ROW / 2.0 + 4.0);
    let class = if r < 1.0 { "faster" } else { "slower" };
    let _ = writeln!(
      out,
      r#"<text class="name" x="{LABELS}" y="{middle:.1}" text-anchor="end">{name}</text><path class="{class}" d="{d}"/><text class="value" x="{value:.1}" y="{middle:.1}" text-anchor="end">{text}</text>"#,
      name = escape(&case.name),
      d = bar(x(1.0), x(r), y + (ROW - BAR) / 2.0),
      value = WIDTH - 16.0,
      text = ratio(r),
    );
    y += ROW;
  }
  out.push_str("</svg>\n");
  out
}
