//! HDDS Naval Convoy Simulator
//!
//! Simulates a convoy of ships being hunted by enemy submarines.
//! Publishes state via HDDS DDS topics for any subscriber to consume.

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use log::{debug, info, warn};
use naval_types::*;
use rand::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

// =============================================================================
// CONFIGURATION
// =============================================================================

const TICK_RATE_HZ: f32 = 30.0;
const SONAR_PING_INTERVAL_S: f32 = 4.0;
const SONAR_PING_SPEED_NM_S: f32 = 0.8;

// Convoy configuration
const CONVOY_SPEED_KTS: f32 = 12.0;
const CONVOY_HEADING_DEG: f32 = 45.0;

// Combat
const TORPEDO_RANGE_NM: f32 = 5.0;

// =============================================================================
// GAME STATE
// =============================================================================

struct GameState {
    ships: HashMap<u32, Ship>,
    submarines: HashMap<u32, Submarine>,
    torpedoes: HashMap<u32, Torpedo>,
    depth_charges: HashMap<u32, DepthCharge>,
    sonar_pings: HashMap<u32, SonarPing>,
    sonar_contacts: HashMap<u32, SonarContact>,
    explosions: Vec<Explosion>,
    events: Vec<CombatEvent>,
    sea_state: SeaState,

    // Counters
    next_ping_id: u32,
    next_torpedo_id: u32,
    next_charge_id: u32,
    next_explosion_id: u32,
    next_event_id: u32,
    next_contact_id: u32,

    // Timers
    last_sonar_ping: Instant,
    simulation_time_us: u64,

    rng: ThreadRng,
}

impl GameState {
    fn new() -> Self {
        let mut state = Self {
            ships: HashMap::new(),
            submarines: HashMap::new(),
            torpedoes: HashMap::new(),
            depth_charges: HashMap::new(),
            sonar_pings: HashMap::new(),
            sonar_contacts: HashMap::new(),
            explosions: Vec::new(),
            events: Vec::new(),
            sea_state: SeaState::default(),
            next_ping_id: 1,
            next_torpedo_id: 1,
            next_charge_id: 1,
            next_explosion_id: 1,
            next_event_id: 1,
            next_contact_id: 1,
            last_sonar_ping: Instant::now(),
            simulation_time_us: 0,
            rng: thread_rng(),
        };

        state.init_convoy();
        state.init_submarines();

        state
    }

    fn init_convoy(&mut self) {
        // Convoy center starts at origin, heading northeast
        let convoy_center = (0.0, 0.0);

        // === FLAGSHIP - Porte-avions Charles de Gaulle ===
        let mut flagship = Ship::new(1, "FS Charles de Gaulle", ship_class::CARRIER, allegiance::FRIENDLY);
        flagship.x_nm = convoy_center.0;
        flagship.y_nm = convoy_center.1;
        flagship.heading_deg = CONVOY_HEADING_DEG;
        flagship.speed_kts = CONVOY_SPEED_KTS;
        flagship.active_sonar = 1;
        self.ships.insert(1, flagship);

        // === SUPPLY SHIPS ===

        // Ravitailleur - left of carrier
        let mut tanker1 = Ship::new(2, "FS Somme", ship_class::TANKER, allegiance::FRIENDLY);
        tanker1.x_nm = convoy_center.0 - 0.6;
        tanker1.y_nm = convoy_center.1 - 0.5;
        tanker1.heading_deg = CONVOY_HEADING_DEG;
        tanker1.speed_kts = CONVOY_SPEED_KTS;
        self.ships.insert(2, tanker1);

        // Cargo logistique - right of carrier
        let mut cargo1 = Ship::new(3, "MV Normandie Express", ship_class::CARGO, allegiance::FRIENDLY);
        cargo1.x_nm = convoy_center.0 + 0.6;
        cargo1.y_nm = convoy_center.1 - 0.5;
        cargo1.heading_deg = CONVOY_HEADING_DEG;
        cargo1.speed_kts = CONVOY_SPEED_KTS;
        self.ships.insert(3, cargo1);

        // === ESCORT SHIPS (Allied NATO Task Force) ===

        // Lead destroyer - point (France - Horizon class)
        let mut destroyer1 = Ship::new(10, "FS Forbin", ship_class::DESTROYER, allegiance::FRIENDLY);
        destroyer1.x_nm = convoy_center.0;
        destroyer1.y_nm = convoy_center.1 + 1.5;
        destroyer1.heading_deg = CONVOY_HEADING_DEG;
        destroyer1.speed_kts = CONVOY_SPEED_KTS;
        destroyer1.active_sonar = 1;
        self.ships.insert(10, destroyer1);

        // Frigate - left flank (France - FREMM Aquitaine class)
        let mut frigate1 = Ship::new(11, "FS Auvergne", ship_class::FRIGATE, allegiance::FRIENDLY);
        frigate1.x_nm = convoy_center.0 - 1.5;
        frigate1.y_nm = convoy_center.1;
        frigate1.heading_deg = CONVOY_HEADING_DEG;
        frigate1.speed_kts = CONVOY_SPEED_KTS;
        frigate1.active_sonar = 1;
        self.ships.insert(11, frigate1);

        // Frigate - right flank (UK - Type 23)
        let mut frigate2 = Ship::new(12, "HMS Northumberland", ship_class::FRIGATE, allegiance::FRIENDLY);
        frigate2.x_nm = convoy_center.0 + 1.5;
        frigate2.y_nm = convoy_center.1;
        frigate2.heading_deg = CONVOY_HEADING_DEG;
        frigate2.speed_kts = CONVOY_SPEED_KTS;
        frigate2.active_sonar = 1;
        self.ships.insert(12, frigate2);

        // Destroyer - rear guard (France - Horizon class)
        let mut destroyer2 = Ship::new(13, "FS Chevalier Paul", ship_class::DESTROYER, allegiance::FRIENDLY);
        destroyer2.x_nm = convoy_center.0;
        destroyer2.y_nm = convoy_center.1 - 2.0;
        destroyer2.heading_deg = CONVOY_HEADING_DEG;
        destroyer2.speed_kts = CONVOY_SPEED_KTS;
        destroyer2.active_sonar = 1;
        self.ships.insert(13, destroyer2);

        info!("Initialized convoy with {} ships", self.ships.len());
    }

    fn init_submarines(&mut self) {
        // Enemy submarine wolf pack

        // Sub 1 - approaching from the northwest
        let mut sub1 = Submarine::new(100, "K-219", allegiance::HOSTILE);
        sub1.x_nm = -4.0;
        sub1.y_nm = 3.0;
        sub1.depth_m = -80.0;
        sub1.heading_deg = 135.0; // Southeast toward convoy
        sub1.speed_kts = 6.0;
        sub1.state = sub_state::SHALLOW;
        self.submarines.insert(100, sub1);

        // Sub 2 - lurking ahead
        let mut sub2 = Submarine::new(101, "K-322", allegiance::HOSTILE);
        sub2.x_nm = 3.0;
        sub2.y_nm = 5.0;
        sub2.depth_m = -120.0;
        sub2.heading_deg = 225.0;
        sub2.speed_kts = 4.0;
        sub2.state = sub_state::DEEP;
        self.submarines.insert(101, sub2);

        // Sub 3 - flanking from east
        let mut sub3 = Submarine::new(102, "K-141 Kursk", allegiance::HOSTILE);
        sub3.x_nm = 5.0;
        sub3.y_nm = 0.0;
        sub3.depth_m = -60.0;
        sub3.heading_deg = 270.0;
        sub3.speed_kts = 8.0;
        sub3.state = sub_state::SHALLOW;
        self.submarines.insert(102, sub3);

        info!("Initialized {} enemy submarines", self.submarines.len());
    }

    fn tick(&mut self, dt_s: f32) {
        self.simulation_time_us += (dt_s * 1_000_000.0) as u64;

        // Clear old explosions
        self.explosions.retain(|e| self.simulation_time_us - e.timestamp_us < 2_000_000);

        // Update entities
        self.update_ships(dt_s);
        self.update_submarines(dt_s);
        self.update_torpedoes(dt_s);
        self.update_depth_charges(dt_s);
        self.update_sonar_pings(dt_s);

        // Sonar detection
        if self.last_sonar_ping.elapsed().as_secs_f32() > SONAR_PING_INTERVAL_S {
            self.emit_sonar_pings();
            self.last_sonar_ping = Instant::now();
        }

        // AI decisions
        self.submarine_ai();
        self.escort_ai();

        // Check collisions
        self.check_torpedo_hits();
        self.check_depth_charge_hits();
    }

    fn update_ships(&mut self, dt_s: f32) {
        for ship in self.ships.values_mut() {
            let speed_nm_s = kts_to_nm_per_sec(ship.speed_kts);
            let heading_rad = ship.heading_deg.to_radians();
            ship.x_nm += heading_rad.sin() * speed_nm_s * dt_s;
            ship.y_nm += heading_rad.cos() * speed_nm_s * dt_s;
            ship.timestamp_us = self.simulation_time_us;
        }
    }

    fn update_submarines(&mut self, dt_s: f32) {
        let convoy_center = self.calculate_convoy_center();

        for sub in self.submarines.values_mut() {
            let speed_nm_s = kts_to_nm_per_sec(sub.speed_kts);
            let heading_rad = sub.heading_deg.to_radians();
            sub.x_nm += heading_rad.sin() * speed_nm_s * dt_s;
            sub.y_nm += heading_rad.cos() * speed_nm_s * dt_s;

            // Update depth based on state
            let target_depth = sub_state::typical_depth_m(sub.state);
            let depth_change_rate = 2.0; // m/s
            if (sub.depth_m - target_depth).abs() > 1.0 {
                if sub.depth_m > target_depth {
                    sub.depth_m -= depth_change_rate * dt_s;
                } else {
                    sub.depth_m += depth_change_rate * dt_s;
                }
            }

            // Update noise level
            sub.noise_level = sub.calculate_noise();

            // Battery drain when submerged and moving
            if sub.depth_m < -5.0 && sub.speed_kts > 2.0 {
                sub.battery_percent -= 0.01 * sub.speed_kts * dt_s;
                sub.battery_percent = sub.battery_percent.max(0.0);
            }

            // Distance to convoy
            sub.distance_to_convoy_nm = distance_nm(sub.x_nm, sub.y_nm, convoy_center.0, convoy_center.1);

            sub.timestamp_us = self.simulation_time_us;
        }
    }

    fn update_torpedoes(&mut self, dt_s: f32) {
        let mut detonated = Vec::new();

        for (id, torpedo) in self.torpedoes.iter_mut() {
            if !torpedo.is_active() {
                continue;
            }

            // Move torpedo
            let speed_nm_s = kts_to_nm_per_sec(torpedo.speed_kts);
            let heading_rad = torpedo.heading_deg.to_radians();
            torpedo.x_nm += heading_rad.sin() * speed_nm_s * dt_s;
            torpedo.y_nm += heading_rad.cos() * speed_nm_s * dt_s;

            // Consume fuel
            torpedo.fuel_remaining -= 0.02 * dt_s;

            // Torpedo guidance (simple homing)
            if torpedo.target_id != 0
                && (torpedo.phase == torpedo_phase::ACQUIRED || torpedo.phase == torpedo_phase::TERMINAL)
            {
                if let Some(ship) = self.ships.get(&torpedo.target_id) {
                    let bearing = bearing_deg(torpedo.x_nm, torpedo.y_nm, ship.x_nm, ship.y_nm);
                    let turn_rate = 10.0 * dt_s;
                    let diff = normalize_angle(bearing - torpedo.heading_deg);
                    if diff < 180.0 {
                        torpedo.heading_deg = normalize_angle(torpedo.heading_deg + turn_rate.min(diff));
                    } else {
                        torpedo.heading_deg = normalize_angle(torpedo.heading_deg - turn_rate.min(360.0 - diff));
                    }
                }
            }

            // Check for fuel exhaustion
            if torpedo.fuel_remaining <= 0.0 {
                torpedo.phase = torpedo_phase::MISSED;
                detonated.push(*id);
            }

            torpedo.timestamp_us = self.simulation_time_us;
        }

        for id in detonated {
            self.torpedoes.remove(&id);
        }
    }

    fn update_depth_charges(&mut self, dt_s: f32) {
        let mut to_detonate = Vec::new();

        for (id, charge) in self.depth_charges.iter_mut() {
            if charge.is_detonated() {
                continue;
            }

            // Sink
            charge.depth_m -= charge.sink_rate_m_s * dt_s;

            // Detonate at set depth
            if charge.depth_m <= charge.set_depth_m {
                charge.detonated = 1;
                to_detonate.push(*id);
            }

            charge.timestamp_us = self.simulation_time_us;
        }

        // Create explosions for detonated charges
        for id in to_detonate {
            if let Some(charge) = self.depth_charges.get(&id) {
                self.create_explosion(
                    charge.x_nm,
                    charge.y_nm,
                    charge.depth_m,
                    0.6,
                    "DEPTH_CHARGE",
                    id,
                    0,
                );
            }
        }

        // Remove old detonated charges
        self.depth_charges.retain(|_, c| !c.is_detonated() || self.simulation_time_us - c.timestamp_us < 3_000_000);
    }

    fn update_sonar_pings(&mut self, dt_s: f32) {
        let mut expired = Vec::new();

        for (id, ping) in self.sonar_pings.iter_mut() {
            ping.radius_nm += SONAR_PING_SPEED_NM_S * dt_s;

            if ping.radius_nm > ping.max_range_nm {
                expired.push(*id);
            }

            ping.timestamp_us = self.simulation_time_us;
        }

        for id in expired {
            self.sonar_pings.remove(&id);
        }
    }

    fn emit_sonar_pings(&mut self) {
        let active_sonar_ships: Vec<_> = self.ships.values()
            .filter(|s| s.is_sonar_active() && s.allegiance == allegiance::FRIENDLY)
            .map(|s| (s.ship_id, s.x_nm, s.y_nm))
            .collect();

        for (ship_id, x, y) in active_sonar_ships {
            let ping = SonarPing {
                ping_id: self.next_ping_id,
                source_id: ship_id,
                sonar_type: sonar_type::ACTIVE_HULL,
                x_nm: x,
                y_nm: y,
                radius_nm: 0.0,
                max_range_nm: 8.0,
                bearing_deg: 0.0,
                beam_width_deg: 360.0,
                timestamp_us: self.simulation_time_us,
            };
            self.sonar_pings.insert(self.next_ping_id, ping);
            self.next_ping_id += 1;

            self.process_sonar_detection(ship_id, x, y, 8.0);
        }
    }

    fn process_sonar_detection(&mut self, detector_id: u32, detector_x: f32, detector_y: f32, range: f32) {
        let mut detections: Vec<(f32, f32, u8, f32, u32, f32, f32, f32, f32)> = Vec::new();

        for sub in self.submarines.values() {
            let dist = distance_nm(detector_x, detector_y, sub.x_nm, sub.y_nm);

            if dist > range {
                continue;
            }

            let dist_factor = 1.0 - (dist / range);
            let depth_factor = if sub.depth_m > -50.0 { 0.9 } else if sub.depth_m > -150.0 { 0.5 } else { 0.2 };
            let noise_factor = sub.noise_level;

            let detection_prob = dist_factor * depth_factor * noise_factor;

            if self.rng.gen::<f32>() < detection_prob {
                let bearing = bearing_deg(detector_x, detector_y, sub.x_nm, sub.y_nm);
                let confidence = if detection_prob > 0.7 {
                    contact_confidence::CONFIRMED
                } else if detection_prob > 0.4 {
                    contact_confidence::PROBABLE
                } else {
                    contact_confidence::POSSIBLE
                };

                detections.push((
                    bearing, dist, confidence, detection_prob, sub.sub_id,
                    sub.depth_m, sub.speed_kts, sub.heading_deg,
                    (1.0 - detection_prob) * 0.5,
                ));
            }
        }

        for (bearing, dist, confidence, detection_prob, sub_id, depth_m, speed_kts, heading_deg, uncertainty) in detections {
            let contact = SonarContact {
                contact_id: self.next_contact_id,
                detector_id,
                bearing_deg: bearing + self.rng.gen_range(-5.0..5.0),
                range_nm: dist + self.rng.gen_range(-uncertainty..uncertainty),
                range_uncertainty_nm: uncertainty,
                depth_estimate_m: depth_m + self.rng.gen_range(-20.0..20.0),
                confidence,
                classification: "SUBMARINE".to_string(),
                signal_strength: detection_prob,
                estimated_speed_kts: speed_kts + self.rng.gen_range(-2.0..2.0),
                estimated_heading_deg: heading_deg + self.rng.gen_range(-15.0..15.0),
                first_detected_us: self.simulation_time_us,
                last_update_us: self.simulation_time_us,
            };

            self.sonar_contacts.insert(self.next_contact_id, contact);
            self.next_contact_id += 1;

            self.add_event(
                "CONTACT",
                format!("Sonar contact! Bearing {:03.0}, range {:.1} nm, confidence {}",
                        bearing, dist, contact_confidence::to_string(confidence)),
                detector_id,
                sub_id,
                1,
            );

            debug!("Submarine {} detected by ship {} at range {:.2} nm", sub_id, detector_id, dist);
        }

        // Expire old contacts
        let time = self.simulation_time_us;
        self.sonar_contacts.retain(|_, c| time - c.last_update_us < 30_000_000);
    }

    fn submarine_ai(&mut self) {
        let _convoy_center = self.calculate_convoy_center();
        let mut torpedoes_to_launch = Vec::new();

        for sub in self.submarines.values_mut() {
            if sub.torpedoes_remaining == 0 {
                continue;
            }

            let dist = sub.distance_to_convoy_nm;

            if dist < TORPEDO_RANGE_NM * 0.8 && self.rng.gen::<f32>() < 0.002 {
                let target = self.ships.values()
                    .filter(|s| s.allegiance == allegiance::FRIENDLY)
                    .min_by(|a, b| {
                        let score_a = distance_nm(sub.x_nm, sub.y_nm, a.x_nm, a.y_nm)
                            * if ship_class::is_military(a.ship_class) { 2.0 } else { 1.0 };
                        let score_b = distance_nm(sub.x_nm, sub.y_nm, b.x_nm, b.y_nm)
                            * if ship_class::is_military(b.ship_class) { 2.0 } else { 1.0 };
                        score_a.partial_cmp(&score_b).unwrap()
                    });

                if let Some(target) = target {
                    let bearing = bearing_deg(sub.x_nm, sub.y_nm, target.x_nm, target.y_nm);
                    torpedoes_to_launch.push((sub.sub_id, target.ship_id, sub.x_nm, sub.y_nm, bearing));
                    sub.torpedoes_remaining -= 1;
                }
            }
        }

        for (launcher_id, target_id, x, y, heading) in torpedoes_to_launch {
            self.launch_torpedo(launcher_id, target_id, allegiance::HOSTILE, x, y, heading);
        }
    }

    fn escort_ai(&mut self) {
        let contacts: Vec<_> = self.sonar_contacts.values()
            .filter(|c| c.confidence == contact_confidence::CONFIRMED || c.confidence == contact_confidence::PROBABLE)
            .map(|c| (c.detector_id, c.bearing_deg, c.range_nm, c.depth_estimate_m))
            .collect();

        for (detector_id, bearing, range, depth) in contacts {
            if let Some(ship) = self.ships.get(&detector_id) {
                if ship.depth_charges_remaining > 0 && self.rng.gen::<f32>() < 0.005 {
                    let bearing_rad = bearing.to_radians();
                    let drop_x = ship.x_nm + bearing_rad.sin() * range * 0.8;
                    let drop_y = ship.y_nm + bearing_rad.cos() * range * 0.8;

                    self.launch_depth_charge(detector_id, drop_x, drop_y, depth);

                    if let Some(ship) = self.ships.get_mut(&detector_id) {
                        ship.depth_charges_remaining -= 1;
                    }
                }
            }
        }
    }

    fn launch_torpedo(&mut self, launcher_id: u32, target_id: u32, alleg: u8, x: f32, y: f32, heading: f32) {
        let mut torpedo = Torpedo::new(self.next_torpedo_id, launcher_id, alleg, x, y, heading);
        torpedo.target_id = target_id;
        torpedo.phase = torpedo_phase::ACQUIRED;
        torpedo.timestamp_us = self.simulation_time_us;

        self.torpedoes.insert(self.next_torpedo_id, torpedo);

        self.add_event(
            "LAUNCH",
            format!("Torpedo in the water! Bearing {:03.0}", heading),
            launcher_id,
            target_id,
            2,
        );

        info!("Torpedo {} launched by {} at target {}", self.next_torpedo_id, launcher_id, target_id);
        self.next_torpedo_id += 1;
    }

    fn launch_depth_charge(&mut self, launcher_id: u32, x: f32, y: f32, depth: f32) {
        let charge = DepthCharge::new(self.next_charge_id, launcher_id, x, y, depth);

        self.depth_charges.insert(self.next_charge_id, charge);

        self.add_event(
            "LAUNCH",
            format!("Depth charge away! Set depth: {:.0}m", depth.abs()),
            launcher_id,
            0,
            1,
        );

        debug!("Depth charge {} dropped at ({:.2}, {:.2})", self.next_charge_id, x, y);
        self.next_charge_id += 1;
    }

    fn check_torpedo_hits(&mut self) {
        let mut hits = Vec::new();

        for (torpedo_id, torpedo) in self.torpedoes.iter() {
            if !torpedo.is_active() {
                continue;
            }

            for ship in self.ships.values() {
                if ship.allegiance == torpedo.allegiance {
                    continue;
                }

                let dist = distance_nm(torpedo.x_nm, torpedo.y_nm, ship.x_nm, ship.y_nm);
                if dist < 0.05 {
                    hits.push((*torpedo_id, ship.ship_id, torpedo.x_nm, torpedo.y_nm));
                    break;
                }
            }
        }

        let mut events_to_add: Vec<(&str, String, u32, u32, u8)> = Vec::new();
        let mut explosions_to_create: Vec<(f32, f32, f32, f32, &str, u32, u32)> = Vec::new();

        for (torpedo_id, ship_id, x, y) in hits {
            if let Some(torpedo) = self.torpedoes.get_mut(&torpedo_id) {
                torpedo.phase = torpedo_phase::DETONATED;
            }

            if let Some(ship) = self.ships.get_mut(&ship_id) {
                ship.hull_integrity -= 0.5;
                ship.hull_integrity = ship.hull_integrity.max(0.0);

                let ship_name = ship.name.clone();
                let destroyed = ship.hull_integrity <= 0.0;
                let hull = ship.hull_integrity;

                events_to_add.push((
                    if destroyed { "KILL" } else { "HIT" },
                    format!("{} hit by torpedo! {}", ship_name,
                            if destroyed { "SHIP SINKING!" } else { "Damage sustained." }),
                    torpedo_id,
                    ship_id,
                    2,
                ));

                warn!("{} hit! Hull integrity: {:.0}%", ship_name, hull * 100.0);
            }

            explosions_to_create.push((x, y, 0.0, 1.0, "TORPEDO_HIT", torpedo_id, ship_id));
        }

        for (event_type, desc, actor, target, priority) in events_to_add {
            self.add_event(event_type, desc, actor, target, priority);
        }
        for (x, y, depth, mag, exp_type, src, victim) in explosions_to_create {
            self.create_explosion(x, y, depth, mag, exp_type, src, victim);
        }

        // Remove ships with 0 hull
        let sunk: Vec<_> = self.ships.iter()
            .filter(|(_, s)| s.hull_integrity <= 0.0)
            .map(|(id, s)| (*id, s.x_nm, s.y_nm, s.name.clone()))
            .collect();

        for (id, x, y, name) in sunk {
            self.ships.remove(&id);
            self.create_explosion(x, y, 0.0, 0.8, "SHIP_SINKING", 0, id);
            warn!("{} has sunk!", name);
        }
    }

    fn check_depth_charge_hits(&mut self) {
        let detonated: Vec<_> = self.depth_charges.iter()
            .filter(|(_, c)| c.is_detonated())
            .map(|(id, c)| (*id, c.x_nm, c.y_nm, c.depth_m, c.blast_radius_m))
            .collect();

        let mut sub_hits: Vec<(u32, u32, String, f32)> = Vec::new();
        let mut sub_updates: Vec<(u32, f32)> = Vec::new();

        for (charge_id, x, y, depth, blast_radius) in detonated {
            for sub in self.submarines.values() {
                let horiz_dist = distance_nm(x, y, sub.x_nm, sub.y_nm);
                let vert_dist = (depth - sub.depth_m).abs();
                let horiz_dist_m = horiz_dist * 1852.0;

                if horiz_dist_m < blast_radius * 2.0 && vert_dist < blast_radius {
                    let damage = 1.0 - (horiz_dist_m / (blast_radius * 2.0));
                    sub_hits.push((charge_id, sub.sub_id, sub.name.clone(), damage));
                    sub_updates.push((sub.sub_id, normalize_angle(sub.heading_deg + 180.0)));
                }
            }
        }

        for (sub_id, new_heading) in sub_updates {
            if let Some(sub) = self.submarines.get_mut(&sub_id) {
                sub.state = sub_state::EMERGENCY;
                sub.speed_kts = 15.0;
                sub.heading_deg = new_heading;
                info!("Submarine {} damaged by depth charge!", sub_id);
            }
        }

        for (charge_id, sub_id, name, damage) in sub_hits {
            self.add_event(
                "HIT",
                format!("Depth charge near miss on {}! Damage: {:.0}%", name, damage * 100.0),
                charge_id,
                sub_id,
                2,
            );
        }
    }

    fn create_explosion(&mut self, x: f32, y: f32, depth: f32, magnitude: f32, explosion_type: &str, source_id: u32, victim_id: u32) {
        let explosion = Explosion {
            explosion_id: self.next_explosion_id,
            x_nm: x,
            y_nm: y,
            depth_m: depth,
            magnitude,
            explosion_type: explosion_type.to_string(),
            source_id,
            victim_id,
            timestamp_us: self.simulation_time_us,
        };

        self.explosions.push(explosion);
        self.next_explosion_id += 1;
    }

    fn add_event(&mut self, event_type: &str, description: String, actor_id: u32, target_id: u32, priority: u8) {
        let event = CombatEvent {
            event_id: self.next_event_id,
            event_type: event_type.to_string(),
            description,
            actor_id,
            target_id,
            priority,
            timestamp_us: self.simulation_time_us,
        };

        self.events.push(event);
        self.next_event_id += 1;

        // Keep only recent events
        if self.events.len() > 100 {
            self.events.remove(0);
        }
    }

    fn calculate_convoy_center(&self) -> (f32, f32) {
        let friendly_ships: Vec<_> = self.ships.values()
            .filter(|s| s.allegiance == allegiance::FRIENDLY)
            .collect();

        if friendly_ships.is_empty() {
            return (0.0, 0.0);
        }

        let sum_x: f32 = friendly_ships.iter().map(|s| s.x_nm).sum();
        let sum_y: f32 = friendly_ships.iter().map(|s| s.y_nm).sum();
        let count = friendly_ships.len() as f32;

        (sum_x / count, sum_y / count)
    }

    /// Build convoy status summary
    fn build_convoy_status(&self) -> ConvoyStatus {
        let center = self.calculate_convoy_center();
        let friendly_count = self.ships.values().filter(|s| s.allegiance == allegiance::FRIENDLY).count();
        let escort_count = self.ships.values()
            .filter(|s| s.allegiance == allegiance::FRIENDLY && ship_class::is_military(s.ship_class))
            .count();

        ConvoyStatus {
            convoy_id: 1,
            convoy_name: "CONVOY ALPHA".to_string(),
            center_x_nm: center.0,
            center_y_nm: center.1,
            base_heading_deg: CONVOY_HEADING_DEG,
            base_speed_kts: CONVOY_SPEED_KTS,
            ship_count: friendly_count as u8,
            escort_count: escort_count as u8,
            status_message: if self.torpedoes.values().any(|t| t.allegiance == allegiance::HOSTILE) {
                "UNDER ATTACK - TORPEDO IN WATER".to_string()
            } else if !self.sonar_contacts.is_empty() {
                "SUBMARINE CONTACT".to_string()
            } else {
                "On course, all ahead standard".to_string()
            },
            timestamp_us: self.simulation_time_us,
        }
    }
}

// =============================================================================
// DDS WRITERS
// =============================================================================

struct DdsWriters {
    ship: hdds::DataWriter<Ship>,
    convoy_status: hdds::DataWriter<ConvoyStatus>,
    submarine: hdds::DataWriter<Submarine>,
    sonar_ping: hdds::DataWriter<SonarPing>,
    sonar_contact: hdds::DataWriter<SonarContact>,
    torpedo: hdds::DataWriter<Torpedo>,
    depth_charge: hdds::DataWriter<DepthCharge>,
    explosion: hdds::DataWriter<Explosion>,
    combat_event: hdds::DataWriter<CombatEvent>,
    sea_state: hdds::DataWriter<SeaState>,
}

impl DdsWriters {
    fn new(participant: &Arc<Participant>) -> Result<Self> {
        let qos = QoS::best_effort();

        Ok(Self {
            ship: participant.topic::<Ship>(topics::SHIP)?
                .writer()
                .qos(qos.clone())
                .build()?,
            convoy_status: participant.topic::<ConvoyStatus>(topics::CONVOY_STATUS)?
                .writer()
                .qos(qos.clone())
                .build()?,
            submarine: participant.topic::<Submarine>(topics::SUBMARINE)?
                .writer()
                .qos(qos.clone())
                .build()?,
            sonar_ping: participant.topic::<SonarPing>(topics::SONAR_PING)?
                .writer()
                .qos(qos.clone())
                .build()?,
            sonar_contact: participant.topic::<SonarContact>(topics::SONAR_CONTACT)?
                .writer()
                .qos(qos.clone())
                .build()?,
            torpedo: participant.topic::<Torpedo>(topics::TORPEDO)?
                .writer()
                .qos(qos.clone())
                .build()?,
            depth_charge: participant.topic::<DepthCharge>(topics::DEPTH_CHARGE)?
                .writer()
                .qos(qos.clone())
                .build()?,
            explosion: participant.topic::<Explosion>(topics::EXPLOSION)?
                .writer()
                .qos(qos.clone())
                .build()?,
            combat_event: participant.topic::<CombatEvent>(topics::COMBAT_EVENT)?
                .writer()
                .qos(qos.clone())
                .build()?,
            sea_state: participant.topic::<SeaState>(topics::SEA_STATE)?
                .writer()
                .qos(qos)
                .build()?,
        })
    }

    /// Publish all current game state to DDS topics
    fn publish(&self, state: &GameState) -> Result<()> {
        // Ships
        for ship in state.ships.values() {
            self.ship.write(ship)?;
        }

        // Submarines (in a real scenario, only show own side)
        for sub in state.submarines.values() {
            self.submarine.write(sub)?;
        }

        // Sonar pings
        for ping in state.sonar_pings.values() {
            self.sonar_ping.write(ping)?;
        }

        // Sonar contacts
        for contact in state.sonar_contacts.values() {
            self.sonar_contact.write(contact)?;
        }

        // Torpedoes
        for torpedo in state.torpedoes.values() {
            self.torpedo.write(torpedo)?;
        }

        // Depth charges
        for charge in state.depth_charges.values() {
            self.depth_charge.write(charge)?;
        }

        // Explosions
        for explosion in &state.explosions {
            self.explosion.write(explosion)?;
        }

        // Recent combat events (last 10)
        for event in state.events.iter().rev().take(10) {
            self.combat_event.write(event)?;
        }

        // Sea state
        self.sea_state.write(&state.sea_state)?;

        // Convoy status
        let convoy_status = state.build_convoy_status();
        self.convoy_status.write(&convoy_status)?;

        Ok(())
    }
}

// =============================================================================
// MAIN
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("===========================================");
    info!("  HDDS Naval Convoy Simulator");
    info!("===========================================");

    // Create HDDS DDS participant
    let participant = Participant::builder("naval_convoy_sim")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    info!("DDS Participant created (domain 0, UDP multicast)");

    // Create writers for all topics
    let writers = DdsWriters::new(&participant)?;
    info!("Created DDS writers for {} topics", 10);

    // Main simulation loop
    let mut state = GameState::new();
    let tick_duration = Duration::from_secs_f32(1.0 / TICK_RATE_HZ);
    let mut last_tick = Instant::now();

    info!("Simulation starting...");
    info!("Convoy: {} ships, heading {:03.0} at {} kts",
          state.ships.len(), CONVOY_HEADING_DEG, CONVOY_SPEED_KTS);
    info!("Threats: {} enemy submarines", state.submarines.len());

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        // Update simulation
        state.tick(dt);

        // Publish all state to DDS topics
        if let Err(e) = writers.publish(&state) {
            warn!("DDS publish error: {}", e);
        }

        // Sleep until next tick
        let elapsed = now.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        }
    }
}
