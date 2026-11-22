# Cloud-Native K8s Discovery Demo

HDDS demonstration showing Kubernetes-style cluster monitoring with:
- Simulated K8s pod workloads publishing CPU/memory/connection metrics
- Live discovery events (pod join/leave lifecycle)
- Auto-scaling logic (scale up >70% CPU, scale down <30%)
- Cross-site communication with relay detection
- TUI dashboard (ratatui)

**What is live and what is not.** `cloud-worker` publishes for real:
`WorkloadMetric` and `DiscoveryEvent` go on the wire. `cloud-monitor` joins the
domain but creates no reader, so everything it draws, the pod grid included, is
generated inside the dashboard. Two of the four topics below,
`cloud/scaling_decision` and `cloud/cross_site_message`, are declared in the
IDL and published by nobody. Wiring the dashboard to the worker topics is open
work.

The join and leave events are published once each, so the worker offers them
with TRANSIENT_LOCAL durability for readers that connect in between. A reader
only receives that history if it REQUESTS the same durability, which a native
DDS reader can do; the `hdds-ws` bridge creates its readers with default QoS,
so a browser still misses a join it was not there for.

## Quick Start

```bash
# Terminal 1: Start the TUI monitor
cargo run --bin cloud-monitor

# Terminal 2+: Start worker pods
cargo run --bin cloud-worker
cargo run --bin cloud-worker -- --pod-name worker-2 --node-name node-b
cargo run --bin cloud-worker -- --pod-name worker-3 --node-name node-c
```

Dashboard controls: `q` quit, `+` scale up, `-` scale down.

## Architecture

```
+--------------+   no reader yet   +-----------------+
| cloud-worker | :...............> |  cloud-monitor  |
| (pod sim x3) |                   | (TUI dashboard) |
+--------------+                   +-----------------+
      |                                   |
      | Publishes on DDS:                 | Generates locally:
      |-- WorkloadMetric (2 Hz)          |-- ScalingDecision (8s)
      +-- DiscoveryEvent (on join/leave) +-- CrossSiteMessage (3-8s)
```

## Topics

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `cloud/workload_metric` | WorkloadMetric | 2 Hz | Pod CPU%, memory MB, connections, throughput |
| `cloud/discovery_event` | DiscoveryEvent | Event | Pod join/leave with discovery method |
| `cloud/cross_site_message` | CrossSiteMessage | 3-8s | Multi-region messages with RTT |
| `cloud/scaling_decision` | ScalingDecision | ~8s | Auto-scaling up/down with reason |

## TUI Dashboard

Every panel below is fed by the dashboard's own simulation, not by the worker
topics; see the note at the top of this file.

- **Pod Grid**: Name, node, CPU% bar chart, memory, connections, throughput, status
- **Discovery Log**: Timestamped join/leave events (color-coded)
- **Scaling Decisions**: Scale up/down history with reasons
- **Cross-Site Stats**: Message count, avg RTT, direct vs relayed

Status colors: green (Running), yellow (HighLoad >50%), red (Critical >80%).

## Simulation Details

- CPU: sinusoidal base + random noise + periodic spikes (~30s intervals)
- Memory: slow growth with small fluctuations (100-500 MB)
- Auto-scaling: avg CPU >70% = scale up, <30% = scale down (3-8 replicas)
- Cross-site routes: us-east-1, eu-west-1, ap-south-1 with 30% relay chance

## Project Structure

```
cloud-k8s/
  idl/cloud_native.idl      IDL type definitions (4 topics)
  cloud-types/               Generated Rust types + CDR codecs
  cloud-worker/              Pod worker simulator
  cloud-monitor/             TUI dashboard (ratatui + crossterm)
  k8s/worker-deployment.yaml Example K8s manifest (documentation)
```

## Dependencies

- Rust + Cargo (for simulator and monitor)
- HDDS (included via workspace)
