//! Smart Factory Simulator
//!
//! Simulates an industrial production line with:
//! - Machines (CNC, welder, assembler, painter, inspector, packager)
//! - Conveyor belts connecting machines
//! - Products moving through the line
//! - Sensors monitoring machine health
//! - Alerts for maintenance and faults

use anyhow::Result;
use factory_types::factory::*;
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;

fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

// DDS topic names
const TOPIC_MACHINE: &str = "factory/machine";
const TOPIC_CONVEYOR: &str = "factory/conveyor";
const TOPIC_PRODUCT: &str = "factory/product";
const TOPIC_ALERT: &str = "factory/alert";
const TOPIC_FACTORY_STATE: &str = "factory/factory_state";

const TICK_RATE_HZ: u64 = 20;

// Production parameters
const PRODUCT_SPAWN_INTERVAL: f32 = 3.0; // seconds between new products
const MACHINE_CYCLE_TIME: f32 = 2.0; // base processing time
const CONVEYOR_SPEED: f32 = 0.3; // position units per second

// ============================================================================
// Simulation state
// ============================================================================

struct MachineSim {
    machine: Machine,
    processing_product: Option<u32>,
    process_timer: f32,
    fault_chance: f32,
    base_temp: f32,
}

impl MachineSim {
    fn new(id: u32, name: &str, mtype: MachineType, gx: i16, gy: i16) -> Self {
        Self {
            machine: Machine {
                machine_id: id,
                name: name.to_string(),
                machine_type: mtype,
                state: MachineState::STATE_IDLE,
                grid_x: gx,
                grid_y: gy,
                temperature_c: 25.0,
                vibration_mm_s: 0.1,
                power_kw: 0.5,
                efficiency_pct: 95,
                items_processed: 0,
                items_rejected: 0,
                cycle_time_s: MACHINE_CYCLE_TIME,
                timestamp_us: 0,
            },
            processing_product: None,
            process_timer: 0.0,
            fault_chance: 0.001,
            base_temp: 25.0,
        }
    }
}

struct ConveyorSim {
    conveyor: Conveyor,
    products: Vec<(u32, f32)>, // (product_id, position 0-1)
}

impl ConveyorSim {
    fn new(id: u32, sx: i16, sy: i16, ex: i16, ey: i16) -> Self {
        Self {
            conveyor: Conveyor {
                conveyor_id: id,
                state: ConveyorState::CONVEYOR_RUNNING,
                start_x: sx,
                start_y: sy,
                end_x: ex,
                end_y: ey,
                speed_m_s: CONVEYOR_SPEED,
                capacity: 5,
                current_items: 0,
                timestamp_us: 0,
            },
            products: Vec::new(),
        }
    }
}

struct ProductSim {
    product: Product,
}

impl ProductSim {
    fn new(id: u32, batch: u32) -> Self {
        Self {
            product: Product {
                product_id: id,
                product_type: ProductType::PRODUCT_RAW,
                quality_pct: 100,
                on_conveyor_id: 0,
                in_machine_id: 0,
                conveyor_position: 0.0,
                batch_id: batch,
                created_us: timestamp_us(),
                timestamp_us: 0,
            },
        }
    }
}

struct AlertSim {
    alert: Alert,
}

/// Factory layout definition
struct FactoryLayout {
    machines: Vec<MachineSim>,
    conveyors: Vec<ConveyorSim>,
    // Machine flow: machine_id -> (output_conveyor_id, next_machine_id)
    machine_flow: Vec<(u32, u32, u32)>,
}

impl FactoryLayout {
    fn create_production_line() -> Self {
        // Create machines in a production line layout
        //
        // Layout (isometric grid):
        //   (0,0) CNC -> (1,0) WELDER -> (2,0) ASSEMBLER
        //                                      |
        //   (2,2) PACKAGER <- (1,2) INSPECTOR <- (0,2) PAINTER
        //
        let machines = vec![
            MachineSim::new(1, "CNC-01", MachineType::MACHINE_CNC, 0, 0),
            MachineSim::new(2, "WELD-01", MachineType::MACHINE_WELDER, 2, 0),
            MachineSim::new(3, "ASSY-01", MachineType::MACHINE_ASSEMBLER, 4, 0),
            MachineSim::new(4, "PAINT-01", MachineType::MACHINE_PAINTER, 4, 2),
            MachineSim::new(5, "QC-01", MachineType::MACHINE_INSPECTOR, 2, 2),
            MachineSim::new(6, "PACK-01", MachineType::MACHINE_PACKAGER, 0, 2),
        ];

        // Conveyors connect machines
        let conveyors = vec![
            ConveyorSim::new(1, 0, 0, 2, 0),   // CNC -> WELDER
            ConveyorSim::new(2, 2, 0, 4, 0),   // WELDER -> ASSEMBLER
            ConveyorSim::new(3, 4, 0, 4, 2),   // ASSEMBLER -> PAINTER
            ConveyorSim::new(4, 4, 2, 2, 2),   // PAINTER -> INSPECTOR
            ConveyorSim::new(5, 2, 2, 0, 2),   // INSPECTOR -> PACKAGER
        ];

        // Flow: (from_machine, output_conveyor, to_machine)
        // 0 means end of line
        let machine_flow = vec![
            (1, 1, 2), // CNC outputs to conveyor 1, goes to WELDER
            (2, 2, 3), // WELDER outputs to conveyor 2, goes to ASSEMBLER
            (3, 3, 4), // ASSEMBLER outputs to conveyor 3, goes to PAINTER
            (4, 4, 5), // PAINTER outputs to conveyor 4, goes to INSPECTOR
            (5, 5, 6), // INSPECTOR outputs to conveyor 5, goes to PACKAGER
            (6, 0, 0), // PACKAGER outputs finished product
        ];

        Self {
            machines,
            conveyors,
            machine_flow,
        }
    }

    fn get_output_conveyor(&self, machine_id: u32) -> Option<u32> {
        self.machine_flow
            .iter()
            .find(|(m, _, _)| *m == machine_id)
            .map(|(_, c, _)| *c)
            .filter(|&c| c != 0)
    }

    fn get_next_machine(&self, machine_id: u32) -> Option<u32> {
        self.machine_flow
            .iter()
            .find(|(m, _, _)| *m == machine_id)
            .map(|(_, _, n)| *n)
            .filter(|&n| n != 0)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Smart Factory Simulator starting...");

    // Create DDS participant
    let participant = Participant::builder("factory_sim")
        .domain_id(99)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    // Create writers
    let machine_writer = participant.create_writer::<Machine>(TOPIC_MACHINE, qos.clone())?;
    let conveyor_writer = participant.create_writer::<Conveyor>(TOPIC_CONVEYOR, qos.clone())?;
    let product_writer = participant.create_writer::<Product>(TOPIC_PRODUCT, qos.clone())?;
    let alert_writer = participant.create_writer::<Alert>(TOPIC_ALERT, qos.clone())?;
    let state_writer = participant.create_writer::<FactoryState>(TOPIC_FACTORY_STATE, qos)?;

    // Create factory layout
    let mut layout = FactoryLayout::create_production_line();

    // Publish initial machine and conveyor data
    for m in &layout.machines {
        let _ = machine_writer.write(&m.machine);
    }
    for c in &layout.conveyors {
        let _ = conveyor_writer.write(&c.conveyor);
    }

    // Product and alert tracking
    let mut products: Vec<ProductSim> = Vec::new();
    let mut alerts: Vec<AlertSim> = Vec::new();
    let mut next_product_id = 1u32;
    let mut next_alert_id = 1u32;
    let mut spawn_timer = 0.0f32;
    let mut products_completed = 0u32;
    let mut products_rejected = 0u32;

    let tick_duration = std::time::Duration::from_micros(1_000_000 / TICK_RATE_HZ);
    let mut last_tick = std::time::Instant::now();
    let mut rng = rand::thread_rng();

    log::info!(
        "Simulation running with {} machines, {} conveyors",
        layout.machines.len(),
        layout.conveyors.len()
    );

    loop {
        let now = std::time::Instant::now();
        let dt = (now - last_tick).as_secs_f32();
        last_tick = now;

        // Spawn new products
        spawn_timer += dt;
        if spawn_timer >= PRODUCT_SPAWN_INTERVAL {
            spawn_timer = 0.0;
            let mut product = ProductSim::new(next_product_id, 1);
            next_product_id += 1;

            // Put product in first machine (CNC)
            product.product.in_machine_id = 1;
            products.push(product);
            log::debug!("Spawned product {}", next_product_id - 1);
        }

        // Collect machine processing results first (to avoid borrow conflicts)
        let mut machine_outputs: Vec<(u32, u32, MachineType, Option<u32>)> = Vec::new(); // (product_id, machine_id, machine_type, output_conveyor)

        // Precompute output conveyors for each machine
        let output_conveyors: Vec<(u32, Option<u32>)> = layout
            .machines
            .iter()
            .map(|m| (m.machine.machine_id, layout.get_output_conveyor(m.machine.machine_id)))
            .collect();

        // Update machines
        for i in 0..layout.machines.len() {
            let msim = &mut layout.machines[i];
            let m = &mut msim.machine;
            let output_conv = output_conveyors.iter().find(|(id, _)| *id == m.machine_id).and_then(|(_, c)| *c);

            // Check for random faults
            if m.state != MachineState::STATE_FAULT && rng.gen::<f32>() < msim.fault_chance {
                m.state = MachineState::STATE_FAULT;
                m.power_kw = 0.0;

                // Create alert
                let alert = AlertSim {
                    alert: Alert {
                        alert_id: next_alert_id,
                        machine_id: m.machine_id,
                        severity: AlertSeverity::ALERT_CRITICAL,
                        message: format!("{} fault detected!", m.name),
                        acknowledged: false,
                        timestamp_us: timestamp_us(),
                    },
                };
                next_alert_id += 1;
                let _ = alert_writer.write(&alert.alert);
                alerts.push(alert);
            }

            // Auto-recover from fault after some time (simplified)
            if m.state == MachineState::STATE_FAULT && rng.gen::<f32>() < 0.01 {
                m.state = MachineState::STATE_IDLE;
            }

            // Check if machine has a product to process
            if msim.processing_product.is_none() {
                // Look for product waiting in this machine
                if let Some(p) = products
                    .iter_mut()
                    .find(|p| p.product.in_machine_id == m.machine_id)
                {
                    if m.state != MachineState::STATE_FAULT {
                        msim.processing_product = Some(p.product.product_id);
                        msim.process_timer = m.cycle_time_s;
                        m.state = MachineState::STATE_WORKING;
                        m.power_kw = 5.0 + rng.gen::<f32>() * 2.0;
                    }
                } else {
                    if m.state == MachineState::STATE_WORKING {
                        m.state = MachineState::STATE_STARVED;
                    } else if m.state != MachineState::STATE_FAULT {
                        m.state = MachineState::STATE_IDLE;
                    }
                    m.power_kw = 0.5;
                }
            }

            // Process product
            if let Some(product_id) = msim.processing_product {
                msim.process_timer -= dt;

                // Update telemetry while working
                m.temperature_c = msim.base_temp + 20.0 + rng.gen::<f32>() * 5.0;
                m.vibration_mm_s = 1.0 + rng.gen::<f32>() * 0.5;

                if msim.process_timer <= 0.0 {
                    // Processing complete
                    m.items_processed += 1;

                    // Queue the output for processing after the loop
                    machine_outputs.push((product_id, m.machine_id, m.machine_type, output_conv));

                    msim.processing_product = None;
                    m.state = MachineState::STATE_IDLE;
                    m.temperature_c = msim.base_temp + rng.gen::<f32>() * 2.0;
                    m.vibration_mm_s = 0.1;
                    m.power_kw = 0.5;
                }
            }

            m.timestamp_us = timestamp_us();
            let _ = machine_writer.write(m);
        }

        // Apply machine outputs to products
        for (product_id, _machine_id, machine_type, output_conv) in machine_outputs {
            if let Some(p) = products.iter_mut().find(|p| p.product.product_id == product_id) {
                // Upgrade product type based on machine
                p.product.product_type = match machine_type {
                    MachineType::MACHINE_CNC => ProductType::PRODUCT_MACHINED,
                    MachineType::MACHINE_WELDER => ProductType::PRODUCT_WELDED,
                    MachineType::MACHINE_ASSEMBLER => ProductType::PRODUCT_ASSEMBLED,
                    MachineType::MACHINE_PAINTER => ProductType::PRODUCT_PAINTED,
                    MachineType::MACHINE_INSPECTOR => ProductType::PRODUCT_INSPECTED,
                    MachineType::MACHINE_PACKAGER => ProductType::PRODUCT_PACKAGED,
                };

                // Quality degradation chance
                if rng.gen::<f32>() < 0.05 {
                    p.product.quality_pct = p.product.quality_pct.saturating_sub(5);
                }

                // Move to output conveyor or finish
                p.product.in_machine_id = 0;
                if let Some(conv_id) = output_conv {
                    p.product.on_conveyor_id = conv_id;
                    p.product.conveyor_position = 0.0;
                } else {
                    // End of line - product completed
                    if p.product.quality_pct >= 80 {
                        products_completed += 1;
                        log::debug!("Product {} completed!", product_id);
                    } else {
                        products_rejected += 1;
                        log::debug!("Product {} rejected (quality {}%)", product_id, p.product.quality_pct);
                    }
                }
            }
        }

        // Update conveyors and move products
        for csim in &mut layout.conveyors {
            let c = &mut csim.conveyor;

            // Count products on this conveyor
            c.current_items = products
                .iter()
                .filter(|p| p.product.on_conveyor_id == c.conveyor_id)
                .count() as u8;

            c.timestamp_us = timestamp_us();
            let _ = conveyor_writer.write(c);
        }

        // Move products on conveyors - first pass: collect movements
        let mut product_moves: Vec<(u32, u32)> = Vec::new(); // (product_id, next_machine_id)

        // First pass: update positions and collect arrivals
        for p in &mut products {
            if p.product.on_conveyor_id > 0 {
                p.product.conveyor_position += CONVEYOR_SPEED * dt;

                // Check if reached end of conveyor
                if p.product.conveyor_position >= 1.0 {
                    p.product.conveyor_position = 1.0; // Clamp

                    // Find which conveyor this is and get next machine
                    let conv_id = p.product.on_conveyor_id;
                    if let Some((from_m, _, _)) = layout
                        .machine_flow
                        .iter()
                        .find(|(_, c, _)| *c == conv_id)
                    {
                        if let Some(next_m) = layout.get_next_machine(*from_m) {
                            product_moves.push((p.product.product_id, next_m));
                        }
                    }
                }
            }
        }

        // Second pass: apply moves (checking machine availability)
        for (product_id, next_machine) in product_moves {
            // Check if next machine is available
            let machine_available = !products
                .iter()
                .any(|op| op.product.in_machine_id == next_machine);

            if machine_available {
                if let Some(p) = products.iter_mut().find(|p| p.product.product_id == product_id) {
                    p.product.on_conveyor_id = 0;
                    p.product.in_machine_id = next_machine;
                    p.product.conveyor_position = 0.0;
                }
            }
        }

        // Publish all products via DDS
        for p in &mut products {
            p.product.timestamp_us = timestamp_us();
            let _ = product_writer.write(&p.product);
        }

        // Remove completed/rejected products
        products.retain(|p| {
            p.product.on_conveyor_id > 0
                || p.product.in_machine_id > 0
                || p.product.product_type != ProductType::PRODUCT_PACKAGED
        });

        // Auto-acknowledge old alerts
        for a in &mut alerts {
            if !a.alert.acknowledged
                && timestamp_us() - a.alert.timestamp_us > 10_000_000
            {
                a.alert.acknowledged = true;
            }
        }
        alerts.retain(|a| !a.alert.acknowledged);

        // Factory state
        let machines_working = layout
            .machines
            .iter()
            .filter(|m| m.machine.state == MachineState::STATE_WORKING)
            .count() as u8;
        let machines_idle = layout
            .machines
            .iter()
            .filter(|m| {
                m.machine.state == MachineState::STATE_IDLE
                    || m.machine.state == MachineState::STATE_STARVED
            })
            .count() as u8;
        let machines_fault = layout
            .machines
            .iter()
            .filter(|m| m.machine.state == MachineState::STATE_FAULT)
            .count() as u8;

        let total_power: f32 = layout.machines.iter().map(|m| m.machine.power_kw).sum();

        let state = FactoryState {
            machines_total: layout.machines.len() as u8,
            machines_working,
            machines_idle,
            machines_fault,
            products_in_progress: products.len() as u32,
            products_completed,
            products_rejected,
            throughput_per_hour: (products_completed as f32) * 3600.0
                / (timestamp_us() as f32 / 1_000_000.0).max(1.0),
            overall_efficiency_pct: (machines_working as f32
                / layout.machines.len() as f32
                * 100.0),
            total_power_kw: total_power,
            active_alerts: alerts.iter().filter(|a| !a.alert.acknowledged).count() as u8,
            timestamp_us: timestamp_us(),
        };

        let _ = state_writer.write(&state);

        tokio::time::sleep(tick_duration).await;
    }
}
