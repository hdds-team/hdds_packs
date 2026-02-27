// CONCEPT DEMO - Simulated data for showcase purposes
//!
//! Mission Recorder - Replay Controller
//!
//! A ratatui TUI that reads a `.lp` (InfluxDB Line Protocol) file and replays
//! mission events with a visual timeline, event log, ASCII map, and statistics.
//!
//! Controls:
//!   Space  - Play / Pause
//!   Left   - Rewind 10 seconds
//!   Right  - Forward 10 seconds
//!   1-4    - Speed: 1x / 2x / 4x / 8x
//!   q/Esc  - Quit

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};
use std::collections::HashMap;
use std::fs;
use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// =============================================================================
// PARSED EVENT
// =============================================================================

/// A parsed Line Protocol entry.
#[derive(Debug, Clone)]
struct LpEvent {
    /// InfluxDB measurement name.
    measurement: String,
    /// Tag key-value pairs.
    tags: HashMap<String, String>,
    /// Field key-value pairs (stored as strings for display).
    fields: HashMap<String, String>,
    /// Timestamp in nanoseconds.
    timestamp_ns: u64,
}

/// Parse a single InfluxDB Line Protocol line.
fn parse_lp_line(line: &str) -> Option<LpEvent> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    // Split: measurement[,tags] fields timestamp
    // We need to be careful with spaces inside quoted strings.
    // Simplified parser: split on the LAST space to get timestamp,
    // then split the remainder on the first unquoted space.
    let parts: Vec<&str> = line.rsplitn(2, ' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let timestamp_str = parts[0];
    let rest = parts[1];

    let timestamp_ns: u64 = timestamp_str.parse().ok()?;

    // Split rest into measurement+tags and fields at the first unquoted space
    let (meas_tags, fields_str) = split_first_unquoted_space(rest)?;

    // Parse measurement and tags
    let (measurement, tags) = parse_measurement_tags(meas_tags);

    // Parse fields
    let fields = parse_fields(fields_str);

    Some(LpEvent {
        measurement,
        tags,
        fields,
        timestamp_ns,
    })
}

/// Split a string at the first space that is not inside double quotes.
fn split_first_unquoted_space(s: &str) -> Option<(&str, &str)> {
    let mut in_quotes = false;
    let mut escaped = false;
    for (i, ch) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if ch == ' ' && !in_quotes {
            return Some((&s[..i], &s[i + 1..]));
        }
    }
    None
}

/// Parse "measurement,tag1=v1,tag2=v2" into (measurement, tags).
fn parse_measurement_tags(s: &str) -> (String, HashMap<String, String>) {
    let mut parts = s.splitn(2, ',');
    let measurement = parts.next().unwrap_or("").replace("\\ ", " ").replace("\\,", ",");
    let mut tags = HashMap::new();
    if let Some(tag_str) = parts.next() {
        for kv in tag_str.split(',') {
            if let Some((k, v)) = kv.split_once('=') {
                tags.insert(
                    k.replace("\\ ", " ").replace("\\,", ",").replace("\\=", "="),
                    v.replace("\\ ", " ").replace("\\,", ",").replace("\\=", "="),
                );
            }
        }
    }
    (measurement, tags)
}

/// Parse "field1=val1,field2=val2" into a HashMap.
fn parse_fields(s: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let mut current_key = String::new();
    let mut current_val = String::new();
    let mut in_key = true;
    let mut in_quotes = false;
    let mut escaped = false;

    for ch in s.chars() {
        if escaped {
            if in_key {
                current_key.push(ch);
            } else {
                current_val.push(ch);
            }
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            // Don't include quotes in the stored value
            continue;
        }
        if ch == '=' && in_key && !in_quotes {
            in_key = false;
            continue;
        }
        if ch == ',' && !in_key && !in_quotes {
            // End of field
            let val = current_val.trim_end_matches('i').to_string();
            fields.insert(current_key.clone(), val);
            current_key.clear();
            current_val.clear();
            in_key = true;
            continue;
        }
        if in_key {
            current_key.push(ch);
        } else {
            current_val.push(ch);
        }
    }

    // Last field
    if !current_key.is_empty() {
        let val = current_val.trim_end_matches('i').to_string();
        fields.insert(current_key, val);
    }

    fields
}

// =============================================================================
// REPLAY STATE
// =============================================================================

struct ReplayState {
    events: Vec<LpEvent>,
    /// Index of the next event to display.
    cursor: usize,
    /// Base timestamp (first event).
    base_ns: u64,
    /// End timestamp (last event).
    end_ns: u64,
    /// Current playback position in nanoseconds relative to base.
    position_ns: u64,
    /// Whether playback is active.
    playing: bool,
    /// Playback speed multiplier.
    speed: f64,
    /// Wall-clock instant of the last frame.
    last_frame: Instant,
    /// Visible event log (last N events).
    visible_log: Vec<String>,
    /// Unit positions for the ASCII map: name -> (lat, lon).
    unit_positions: HashMap<String, (f64, f64)>,
    /// Total recording duration in seconds.
    duration_s: f64,
}

impl ReplayState {
    fn load(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let mut events: Vec<LpEvent> = content
            .lines()
            .filter_map(parse_lp_line)
            .collect();

        // Sort by timestamp
        events.sort_by_key(|e| e.timestamp_ns);

        if events.is_empty() {
            return Err("No events found in recording file".into());
        }

        let base_ns = events[0].timestamp_ns;
        let end_ns = events.last().map(|e| e.timestamp_ns).unwrap_or(base_ns);
        let duration_s = (end_ns - base_ns) as f64 / 1_000_000_000.0;

        Ok(Self {
            events,
            cursor: 0,
            base_ns,
            end_ns,
            position_ns: 0,
            playing: false,
            speed: 1.0,
            last_frame: Instant::now(),
            visible_log: Vec::new(),
            unit_positions: HashMap::new(),
            duration_s,
        })
    }

    fn current_seconds(&self) -> f64 {
        self.position_ns as f64 / 1_000_000_000.0
    }

    fn tick(&mut self) {
        if !self.playing {
            self.last_frame = Instant::now();
            return;
        }

        let now = Instant::now();
        let wall_dt = now.duration_since(self.last_frame);
        self.last_frame = now;

        let advance_ns = (wall_dt.as_nanos() as f64 * self.speed) as u64;
        self.position_ns = self.position_ns.saturating_add(advance_ns);

        // Cap at end
        let max_ns = self.end_ns.saturating_sub(self.base_ns);
        if self.position_ns > max_ns {
            self.position_ns = max_ns;
            self.playing = false;
        }

        // Advance cursor and collect events
        let target_ts = self.base_ns + self.position_ns;
        while self.cursor < self.events.len() && self.events[self.cursor].timestamp_ns <= target_ts {
            let evt = &self.events[self.cursor];
            let rel_s = (evt.timestamp_ns - self.base_ns) as f64 / 1_000_000_000.0;

            let source = evt.tags.get("source").cloned().unwrap_or_default();
            let etype = evt.tags.get("type").cloned()
                .or_else(|| evt.tags.get("phase").cloned())
                .unwrap_or_else(|| evt.measurement.clone());

            let log_line = format!(
                "{:>6.1}s  [{:<8}] {:<10} {}",
                rel_s,
                source,
                etype,
                evt.measurement,
            );
            self.visible_log.push(log_line);

            // Keep log bounded
            if self.visible_log.len() > 200 {
                self.visible_log.remove(0);
            }

            // Update unit position from position events
            if evt.measurement == "mission_events" {
                if let (Some(lat_s), Some(lon_s)) =
                    (evt.fields.get("latitude"), evt.fields.get("longitude"))
                {
                    if let (Ok(lat), Ok(lon)) = (lat_s.parse::<f64>(), lon_s.parse::<f64>()) {
                        if !source.is_empty() {
                            self.unit_positions.insert(source.clone(), (lat, lon));
                        }
                    }
                }
            }

            self.cursor += 1;
        }
    }

    fn rewind(&mut self, secs: f64) {
        let rewind_ns = (secs * 1_000_000_000.0) as u64;
        self.position_ns = self.position_ns.saturating_sub(rewind_ns);

        // Reset cursor to find correct position
        let target_ts = self.base_ns + self.position_ns;
        self.cursor = self.events.partition_point(|e| e.timestamp_ns <= target_ts);
    }

    fn forward(&mut self, secs: f64) {
        let fwd_ns = (secs * 1_000_000_000.0) as u64;
        let max_ns = self.end_ns.saturating_sub(self.base_ns);
        self.position_ns = (self.position_ns + fwd_ns).min(max_ns);
    }
}

// =============================================================================
// UI RENDERING
// =============================================================================

fn render_ui(frame: &mut Frame, state: &ReplayState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(3),  // Timeline
            Constraint::Length(3),  // Controls
            Constraint::Min(10),   // Main area
            Constraint::Length(5), // Stats
        ])
        .split(frame.area());

    // -- Title ---------------------------------------------------------------
    let title = Paragraph::new(Line::from(vec![
        Span::styled(" HDDS Mission Replay ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(
            if state.playing { "PLAYING" } else { "PAUSED" },
            Style::default().fg(if state.playing { Color::Green } else { Color::Yellow }),
        ),
        Span::raw(format!(" | Speed: {}x", state.speed)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, outer[0]);

    // -- Timeline bar --------------------------------------------------------
    let progress = if state.duration_s > 0.0 {
        (state.current_seconds() / state.duration_s).min(1.0)
    } else {
        0.0
    };
    let timeline = Gauge::default()
        .block(Block::default().title(" Timeline ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .ratio(progress)
        .label(format!(
            "{:02}:{:02} / {:02}:{:02}",
            state.current_seconds() as u64 / 60,
            state.current_seconds() as u64 % 60,
            state.duration_s as u64 / 60,
            state.duration_s as u64 % 60,
        ));
    frame.render_widget(timeline, outer[1]);

    // -- Controls ------------------------------------------------------------
    let controls = Paragraph::new(Line::from(vec![
        Span::styled(" [Space]", Style::default().fg(Color::Yellow)),
        Span::raw(" Play/Pause  "),
        Span::styled("[<-]", Style::default().fg(Color::Yellow)),
        Span::raw(" -10s  "),
        Span::styled("[->]", Style::default().fg(Color::Yellow)),
        Span::raw(" +10s  "),
        Span::styled("[1-4]", Style::default().fg(Color::Yellow)),
        Span::raw(" Speed  "),
        Span::styled("[q]", Style::default().fg(Color::Red)),
        Span::raw(" Quit"),
    ]))
    .block(Block::default().borders(Borders::ALL).title(" Controls "));
    frame.render_widget(controls, outer[2]);

    // -- Main area: Event Log (left) + ASCII Map (right) ---------------------
    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(outer[3]);

    // Event log
    let log_items: Vec<ListItem> = state
        .visible_log
        .iter()
        .rev()
        .take(main_area[0].height.saturating_sub(2) as usize)
        .rev()
        .map(|line| {
            let style = if line.contains("contact") {
                Style::default().fg(Color::Red)
            } else if line.contains("comm") {
                Style::default().fg(Color::Yellow)
            } else if line.contains("mission_status") {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Span::styled(line.clone(), style))
        })
        .collect();
    let event_log = List::new(log_items)
        .block(Block::default().borders(Borders::ALL).title(" Event Log "));
    frame.render_widget(event_log, main_area[0]);

    // ASCII map
    let map_text = render_ascii_map(&state.unit_positions, main_area[1]);
    let map_widget = Paragraph::new(map_text)
        .block(Block::default().borders(Borders::ALL).title(" Unit Positions "));
    frame.render_widget(map_widget, main_area[1]);

    // -- Stats ---------------------------------------------------------------
    let stats = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Total Events: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", state.events.len()),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled("Duration: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.1}s", state.duration_s),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled("Current: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.1}s", state.current_seconds()),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled("Speed: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}x", state.speed),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled("Cursor: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}/{}", state.cursor, state.events.len()),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Units: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state
                    .unit_positions
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Stats "));
    frame.render_widget(stats, outer[4]);
}

/// Render a simple ASCII grid map of unit positions.
fn render_ascii_map(
    positions: &HashMap<String, (f64, f64)>,
    area: Rect,
) -> Vec<Line<'static>> {
    let map_w = area.width.saturating_sub(2) as usize;
    let map_h = area.height.saturating_sub(2) as usize;

    if positions.is_empty() || map_w < 5 || map_h < 3 {
        return vec![Line::from("  Waiting for position data...")];
    }

    // Find bounding box
    let mut min_lat = f64::MAX;
    let mut max_lat = f64::MIN;
    let mut min_lon = f64::MAX;
    let mut max_lon = f64::MIN;

    for (lat, lon) in positions.values() {
        min_lat = min_lat.min(*lat);
        max_lat = max_lat.max(*lat);
        min_lon = min_lon.min(*lon);
        max_lon = max_lon.max(*lon);
    }

    // Add margin
    let margin = 0.002;
    min_lat -= margin;
    max_lat += margin;
    min_lon -= margin;
    max_lon += margin;

    let lat_range = (max_lat - min_lat).max(0.001);
    let lon_range = (max_lon - min_lon).max(0.001);

    // Build grid
    let mut grid: Vec<Vec<char>> = vec![vec!['.'; map_w]; map_h];

    // Place units
    let mut labels: Vec<(usize, usize, String)> = Vec::new();
    for (name, (lat, lon)) in positions {
        let col = ((lon - min_lon) / lon_range * (map_w as f64 - 1.0)) as usize;
        let row = ((max_lat - lat) / lat_range * (map_h as f64 - 1.0)) as usize;
        let col = col.min(map_w.saturating_sub(1));
        let row = row.min(map_h.saturating_sub(1));

        grid[row][col] = '*';
        labels.push((row, col, name.chars().next().unwrap_or('?').to_string()));
    }

    // Render to Lines
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(map_h);
    for (r, row) in grid.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let row_str: String = row.iter().collect();

        // Check if any label is on this row
        let row_labels: Vec<&(usize, usize, String)> =
            labels.iter().filter(|(lr, _, _)| *lr == r).collect();

        if row_labels.is_empty() {
            spans.push(Span::styled(row_str, Style::default().fg(Color::DarkGray)));
        } else {
            // Color the unit markers
            let mut chars: Vec<(char, Style)> = row_str
                .chars()
                .map(|c| (c, Style::default().fg(Color::DarkGray)))
                .collect();

            for (_, c, label) in &row_labels {
                if *c < chars.len() {
                    chars[*c] = (
                        label.chars().next().unwrap_or('*'),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    );
                }
            }

            for (ch, style) in chars {
                spans.push(Span::styled(ch.to_string(), style));
            }
        }

        lines.push(Line::from(spans));
    }

    lines
}

// =============================================================================
// MAIN
// =============================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Determine input file
    let args: Vec<String> = std::env::args().collect();
    let input_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("mission_recording.lp")
    };

    if !input_path.exists() {
        eprintln!(
            "Recording file not found: {}\n\
             Run recorder-sim and recorder-sink first to generate a recording,\n\
             or pass a .lp file as argument.",
            input_path.display()
        );
        std::process::exit(1);
    }

    // Load events
    let mut state = ReplayState::load(&input_path)?;
    eprintln!(
        "Loaded {} events, duration {:.1}s",
        state.events.len(),
        state.duration_s
    );

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Main loop at ~30 fps
    let frame_duration = Duration::from_millis(33);

    loop {
        // Handle input
        if event::poll(Duration::from_millis(5))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char(' ') => {
                        state.playing = !state.playing;
                        state.last_frame = Instant::now();
                    }
                    KeyCode::Left => state.rewind(10.0),
                    KeyCode::Right => state.forward(10.0),
                    KeyCode::Char('1') => state.speed = 1.0,
                    KeyCode::Char('2') => state.speed = 2.0,
                    KeyCode::Char('3') => state.speed = 4.0,
                    KeyCode::Char('4') => state.speed = 8.0,
                    _ => {}
                }
            }
        }

        // Advance playback
        state.tick();

        // Render
        terminal.draw(|frame| render_ui(frame, &state))?;

        // Rate limit
        let elapsed = state.last_frame.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }

    // Cleanup terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
