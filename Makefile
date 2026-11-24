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

HDDSGEN ?= hddsgen

# One generated file from its IDL. Creates the output directory, skips the demo when its
# IDL is absent (untracked demo trees), and leaves the file untouched when the content is
# unchanged so cargo does not rebuild for nothing.
define GEN
if [ -f $(1) ]; then mkdir -p $(dir $(2)) && $(HDDSGEN) gen rust $(1) -o $(2).tmp >/dev/null && { cmp -s $(2).tmp $(2) && rm -f $(2).tmp || mv $(2).tmp $(2); } && echo "Generated: $(2)"; else echo "Skipped (no IDL): $(1)"; fi
endef

# ---- IDL Code Generation ----
# Format: hddsgen gen rust <idl> -o <output>
# Manual types (air-defense-radar, naval-convoy) use #[derive(DDS)] - no codegen needed

CODEGEN_TARGETS = \
	gen-arcade-shooter \
	gen-artillery \
	gen-cloud \
	gen-convoy \
	gen-drone \
	gen-f1tenth \
	gen-fort-knox \
	gen-hot-reload \
	gen-lidar \
	gen-mission \
	gen-nat-wan \
	gen-predictive \
	gen-railway \
	gen-robot-swarm \
	gen-rpc \
	gen-run-and-gun \
	gen-satellite \
	gen-smart-factory \
	gen-tsn \
	gen-uds \
	gen-xrce

.PHONY: check gen gen-build build clean $(CODEGEN_TARGETS)

check:
	@python3 scripts/check-no-external-paths.py .

gen: $(CODEGEN_TARGETS)
	@echo "=== All types regenerated ==="

gen-arcade-shooter:
	@$(call GEN,arcade-shooter/idl/shooter.idl,arcade-shooter/shooter-types/src/lib.rs)

gen-artillery:
	@$(call GEN,artillery-fire-control/idl/artillery.idl,artillery-fire-control/arty-types/src/generated.rs)

gen-cloud:
	@$(call GEN,cloud-k8s/idl/cloud_native.idl,cloud-k8s/cloud-types/src/generated.rs)

gen-convoy:
	@$(call GEN,convoy-escort/idl/convoy.idl,convoy-escort/convoy-types/src/generated.rs)

gen-drone:
	@$(call GEN,drone-racing/idl/racing.idl,drone-racing/racing-types/src/lib.rs)

gen-f1tenth:
	@$(call GEN,f1tenth-racing/idl/f1tenth.idl,f1tenth-racing/f1tenth-types/src/lib.rs)

gen-fort-knox:
	@$(call GEN,fort-knox-security/idl/fort_knox.idl,fort-knox-security/knox-types/src/generated.rs)

gen-hot-reload:
	@$(call GEN,hot-reload-cockpit/idl/cockpit.idl,hot-reload-cockpit/cockpit-types/src/generated.rs)

gen-lidar:
	@$(call GEN,lidar-mapping/idl/lidar.idl,lidar-mapping/lidar-types/src/lib.rs)

gen-mission:
	@$(call GEN,mission-recorder/idl/mission.idl,mission-recorder/recorder-types/src/generated.rs)

gen-nat-wan:
	@$(call GEN,nat-wan-topology/idl/wan_link.idl,nat-wan-topology/wan-types/src/generated.rs)

gen-predictive:
	@$(call GEN,predictive-maintenance/idl/maintenance.idl,predictive-maintenance/sensor-types/src/lib.rs)

gen-railway:
	@$(call GEN,railway-network/idl/railway.idl,railway-network/railway-types/src/lib.rs)

gen-robot-swarm:
	@$(call GEN,robot-swarm/idl/swarm.idl,robot-swarm/swarm-types/src/lib.rs)

gen-rpc:
	@$(call GEN,rpc-service-mesh/idl/service_mesh.idl,rpc-service-mesh/fleet-types/src/generated.rs)

gen-run-and-gun:
	@$(call GEN,run-and-gun-arena/idl/arena.idl,run-and-gun-arena/arena-types/src/lib.rs)

gen-satellite:
	@$(call GEN,satellite-ground-station/idl/satellite.idl,satellite-ground-station/sat-types/src/generated.rs)

gen-smart-factory:
	@$(call GEN,smart-factory/idl/factory.idl,smart-factory/factory-types/src/lib.rs)

gen-tsn:
	@$(call GEN,tsn-automotive/idl/tsn_vehicle.idl,tsn-automotive/tsn-types/src/generated.rs)

gen-uds:
	@$(call GEN,uds-fleet-diag/idl/huds_diag.idl,uds-fleet-diag/diag-types/src/generated.rs)

gen-xrce:
	@$(call GEN,xrce-micro-bridge/idl/xrce_bridge.idl,xrce-micro-bridge/xrce-types/src/generated.rs)


# ---- Build all demos ----

DEMOS = \
	air-defense-radar \
	arcade-shooter \
	artillery-fire-control \
	cloud-k8s \
	convoy-escort \
	drone-racing \
	f1tenth-racing \
	fort-knox-security \
	hdds-remote-sensor/pi-gateway \
	hdds-remote-sensor/web-viewer \
	hot-reload-cockpit \
	lidar-mapping \
	mission-recorder \
	nat-wan-topology \
	naval-convoy \
	predictive-maintenance \
	railway-network \
	robot-swarm \
	rpc-service-mesh \
	run-and-gun-arena \
	satellite-ground-station \
	smart-factory \
	tsn-automotive \
	uds-fleet-diag \
	xrce-micro-bridge

# The generated code is committed, so a build needs no generator. Run `make gen`
# to refresh it after changing an IDL file.
build:
	@ok=0; fail=0; skip=0; skipped=""; \
	for d in $(DEMOS); do \
		if [ -f $$d/Cargo.toml ]; then \
			printf "  Building %-30s" "$$d..."; \
			if cargo build --locked --manifest-path $$d/Cargo.toml; then \
				echo " OK"; \
				ok=$$((ok + 1)); \
			else \
				echo " FAIL"; \
				fail=$$((fail + 1)); \
			fi; \
		else \
			skip=$$((skip + 1)); \
			skipped="$$skipped $$d"; \
		fi; \
	done; \
	echo "=== $$ok OK, $$fail FAIL, $$skip skipped ==="; \
	if [ $$skip -gt 0 ]; then \
		echo "    no root Cargo.toml, nothing attempted:$$skipped"; \
	fi

gen-build: gen build

clean:
	@for d in $(DEMOS); do \
		if [ -d $$d/target ]; then \
			echo "  Cleaning $$d"; \
			rm -rf $$d/target; \
		fi; \
	done
	@echo "=== Clean done ==="
