# RPC Service Mesh Demo

HDDS demonstration showing a DDS-RPC pattern for fleet management microservices:
- 4 services: Vehicle tracking, Route computation, Maintenance tickets, Alert handling
- Typed request/reply dispatch over DDS pub/sub topics
- Service discovery via heartbeat registration
- Simulated vehicle movement, ticket lifecycle, and alert auto-acknowledgment
- GTK4 service browser with discovery, request editor, and mesh overview

**What is live and what is not.** `fleet-services` runs the four services and
their request/reply topics on DDS. `fleet-explorer` joins no domain at all: the
service tree, the participant count and the topic table are fixed values, and
the request editor builds its reply in the viewer process, so the time it
reports is how long that took and not a round trip. Wiring the explorer to the
registry and reply topics is open work.

## Quick Start

```bash
# Terminal 1: Start all 4 services
cargo run --release --bin fleet-services

# Terminal 2: Start the GTK4 explorer
cargo run --release --bin fleet-explorer
```

## Architecture

```
+-------------------+     DDS UDP      +-------------------+
|  fleet-services   | <--------------> |  fleet-explorer   |
|  (4 RPC services) |                  |  (GTK4 browser)   |
+-------------------+                  +-------------------+
      |
      | Services:
      |-- VehicleService    (RegisterVehicle, LocateVehicle)
      |-- RouteService      (ComputeRoute via Haversine)
      |-- MaintenanceService (CreateTicket, GetTicketStatus)
      +-- AlertService      (RaiseAlert, auto-ack after 30s)
```

## Topics

14 DDS topics total: 1 registry + 13 RPC request/reply pairs.

| Service | Methods | Request/Reply Topics |
|---------|---------|---------------------|
| Registry | - | `rpc/ServiceRegistry` (heartbeat every 10s) |
| VehicleService | RegisterVehicle, LocateVehicle | 4 topics |
| RouteService | ComputeRoute | 2 topics |
| MaintenanceService | CreateTicket, GetTicketStatus | 4 topics |
| AlertService | RaiseAlert | 2 topics |

Topic naming: `rpc/{Service}/{Method}/Request` and `.../Reply`

## RPC Services

- **VehicleService**: 10 seeded vehicles around Paris, position drift simulation, 5 states (IDLE/EN_ROUTE/LOADING/MAINTENANCE/VEHICLE_EMERGENCY)
- **RouteService**: Haversine great-circle distance, ETA with toll avoidance factor (1.3x)
- **MaintenanceService**: Ticket FSM (OPEN -> IN_PROGRESS -> RESOLVED -> CLOSED)
- **AlertService**: Severity levels (INFO/WARNING/CRITICAL/ALERT_EMERGENCY), auto-ack after 30s

## GTK4 Explorer (3 tabs)

All three tabs render fixed values; see the note at the top of this file.

- **Service Discovery**: TreeView of services with health status, method listing
- **RPC Test Panel**: Service/method selector, JSON request editor, send button, reply built in the viewer
- **Mesh Overview**: ASCII topology diagram, topic statistics table

## Project Structure

```
rpc-service-mesh/
  idl/service_mesh.idl      IDL definitions (3 enums, 13 topic structs)
  fleet-types/               Generated Rust types + CDR codecs
  fleet-services/            4 concurrent RPC service threads
  fleet-explorer/            GTK4 service browser
```

## Dependencies

- Rust + Cargo
- GTK4 (for fleet-explorer)
- HDDS (included via workspace)
