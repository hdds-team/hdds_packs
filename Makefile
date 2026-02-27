# HDDS Demos - Build & Codegen Makefile
#
# Usage:
#   make gen          Regenerate all IDL types
#   make build        Build all demos
#   make gen-build    Regenerate + build
#   make clean        Clean all build artifacts
#
# Requires:
#   - hddsgen binary (set HDDSGEN env var or build from hdds_gen repo)
#   - Rust toolchain

HDDSGEN ?= /projects/public/hdds_gen/target/release/hddsgen

# ---- IDL Code Generation ----
# Format: hddsgen gen rust <idl> -o <output>
# Manual types (air-defense-radar, naval-convoy) use #[derive(DDS)] - no codegen needed

CODEGEN_TARGETS = \
	gen-artillery \
	gen-cloud \
	gen-convoy \
	gen-drone \
	gen-f1tenth \
	gen-fort-knox \
	gen-hot-reload \
	gen-lidar \
	gen-metal-slug \
	gen-mission \
	gen-nat-wan \
	gen-predictive \
	gen-railway \
	gen-robot-swarm \
	gen-rpc \
	gen-satellite \
	gen-sentinel-demo \
	gen-sentinel-home \
	gen-smart-factory \
	gen-space-invaders \
	gen-tsn \
	gen-uds \
	gen-xrce

.PHONY: gen gen-build build clean $(CODEGEN_TARGETS)

gen: $(CODEGEN_TARGETS)
	@echo "=== All types regenerated ==="

gen-artillery:
	$(HDDSGEN) gen rust artillery-fire-control/idl/artillery.idl -o artillery-fire-control/arty-types/src/generated.rs

gen-cloud:
	$(HDDSGEN) gen rust cloud-k8s/idl/cloud_native.idl -o cloud-k8s/cloud-types/src/generated.rs

gen-convoy:
	$(HDDSGEN) gen rust convoy-escort/idl/convoy.idl -o convoy-escort/convoy-types/src/generated.rs

gen-drone:
	$(HDDSGEN) gen rust drone-racing/idl/racing.idl -o drone-racing/racing-types/src/lib.rs

gen-f1tenth:
	$(HDDSGEN) gen rust f1tenth-racing/idl/f1tenth.idl -o f1tenth-racing/f1tenth-types/src/lib.rs

gen-fort-knox:
	$(HDDSGEN) gen rust fort-knox-security/idl/fort_knox.idl -o fort-knox-security/knox-types/src/generated.rs

gen-hot-reload:
	$(HDDSGEN) gen rust hot-reload-cockpit/idl/cockpit.idl -o hot-reload-cockpit/cockpit-types/src/generated.rs

gen-lidar:
	$(HDDSGEN) gen rust lidar-mapping/idl/lidar.idl -o lidar-mapping/lidar-types/src/lib.rs

gen-metal-slug:
	$(HDDSGEN) gen rust metal-slug-arena/idl/arena.idl -o metal-slug-arena/arena-types/src/lib.rs

gen-mission:
	$(HDDSGEN) gen rust mission-recorder/idl/mission.idl -o mission-recorder/recorder-types/src/generated.rs

gen-nat-wan:
	$(HDDSGEN) gen rust nat-wan-topology/idl/wan_link.idl -o nat-wan-topology/wan-types/src/generated.rs

gen-predictive:
	$(HDDSGEN) gen rust predictive-maintenance/idl/maintenance.idl -o predictive-maintenance/sensor-types/src/lib.rs

gen-railway:
	$(HDDSGEN) gen rust railway-network/idl/railway.idl -o railway-network/railway-types/src/lib.rs

gen-robot-swarm:
	$(HDDSGEN) gen rust robot-swarm/idl/swarm.idl -o robot-swarm/swarm-types/src/lib.rs

gen-rpc:
	$(HDDSGEN) gen rust rpc-service-mesh/idl/service_mesh.idl -o rpc-service-mesh/fleet-types/src/generated.rs

gen-satellite:
	$(HDDSGEN) gen rust satellite-ground-station/idl/satellite.idl -o satellite-ground-station/sat-types/src/generated.rs

gen-sentinel-demo:
	$(HDDSGEN) gen rust sentinel-demo/idl/sentinel.idl -o sentinel-demo/sentinel-types/src/generated.rs

gen-sentinel-home:
	$(HDDSGEN) gen rust sentinel-home/idl/security.idl -o sentinel-home/sentinel-types/rust/src/lib.rs

gen-smart-factory:
	$(HDDSGEN) gen rust smart-factory/idl/factory.idl -o smart-factory/factory-types/src/lib.rs

gen-space-invaders:
	$(HDDSGEN) gen rust space-invaders/idl/invaders.idl -o space-invaders/invaders-types/src/lib.rs

gen-tsn:
	$(HDDSGEN) gen rust tsn-automotive/idl/tsn_vehicle.idl -o tsn-automotive/tsn-types/src/generated.rs

gen-uds:
	$(HDDSGEN) gen rust uds-fleet-diag/idl/huds_diag.idl -o uds-fleet-diag/diag-types/src/generated.rs

gen-xrce:
	$(HDDSGEN) gen rust xrce-micro-bridge/idl/xrce_bridge.idl -o xrce-micro-bridge/xrce-types/src/generated.rs


# ---- Build all demos ----

DEMOS = \
	air-defense-radar \
	artillery-fire-control \
	cloud-k8s \
	convoy-escort \
	drone-racing \
	f1tenth-racing \
	fort-knox-security \
	hdds-remote-sensor \
	hot-reload-cockpit \
	lidar-mapping \
	metal-slug-arena \
	mission-recorder \
	nat-wan-topology \
	naval-convoy \
	predictive-maintenance \
	railway-network \
	robot-swarm \
	rpc-service-mesh \
	satellite-ground-station \
	sentinel-demo \
	sentinel-home \
	smart-factory \
	space-invaders \
	tsn-automotive \
	uds-fleet-diag \
	xrce-micro-bridge

build:
	@ok=0; fail=0; \
	for d in $(DEMOS); do \
		if [ -f $$d/Cargo.toml ]; then \
			printf "  Building %-30s" "$$d..."; \
			if cargo build --manifest-path $$d/Cargo.toml 2>/dev/null; then \
				echo " OK"; \
				ok=$$((ok + 1)); \
			else \
				echo " FAIL"; \
				fail=$$((fail + 1)); \
			fi; \
		fi; \
	done; \
	echo "=== $$ok OK, $$fail FAIL ==="

gen-build: gen build

clean:
	@for d in $(DEMOS); do \
		if [ -d $$d/target ]; then \
			echo "  Cleaning $$d"; \
			rm -rf $$d/target; \
		fi; \
	done
	@echo "=== Clean done ==="
