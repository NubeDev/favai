# favai — developer Makefile
#
# Targets:
#   make build         Build favai-cli (debug profile).
#   make release       Build favai-cli (release profile).
#   make test          Run the workspace tests.
#   make start         Start favai as a detached background daemon.
#   make stop          Stop the running daemon (if any).
#   make status        Report daemon liveness.
#   make restart       stop -> build -> start. Daemon stays detached.
#   make dev           build -> stop -> run in foreground, tailing logs.
#                      Ctrl-C exits cleanly (and shuts down the agent).
#   make logs          tail -f the daemon log file.
#   make doctor        Show MCP host wiring across all hosts and scopes.
#   make demo          Run the bundled end-to-end demo script.
#   make install       Build (release), install binary to PREFIX, and
#                      install + enable a systemd --user service so favai
#                      auto-starts on login and survives reboots.
#   make install-clean Stop & disable the service, remove the unit file
#                      and the installed binary.
#   make clean         cargo clean.
#
# Override the binary or config path:
#   make start FAVAI=./target/release/favai
#   make dev   CONFIG=/tmp/my-config.toml
#   make install PREFIX=/usr/local           # system-wide binary path

CARGO    ?= cargo
PROFILE  ?= debug
ifeq ($(PROFILE),release)
CARGO_PROFILE_FLAG := --release
else
CARGO_PROFILE_FLAG :=
endif

FAVAI    ?= $(CURDIR)/target/$(PROFILE)/favai
CONFIG   ?= $(HOME)/.config/starter/favai/config.toml
LOG_FILE ?= $(HOME)/.config/starter/favai/favai.log

# `make install` targets — installs a per-user systemd unit. No sudo
# required. To survive logouts/reboots on a headless box, also run
# (once, as root):
#     loginctl enable-linger $$USER
PREFIX        ?= $(HOME)/.local
BINDIR        ?= $(PREFIX)/bin
INSTALL_BIN   := $(BINDIR)/favai
SYSTEMD_DIR   ?= $(HOME)/.config/systemd/user
UNIT_NAME     ?= favai.service
UNIT_PATH     := $(SYSTEMD_DIR)/$(UNIT_NAME)

.PHONY: build release test start stop kill status restart dev logs doctor demo \
        install install-clean clean help

help:
	@awk 'BEGIN{FS=":.*##"} /^[a-zA-Z_-]+:.*##/ {printf "  %-10s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build favai-cli (debug)
	$(CARGO) build -p favai-cli $(CARGO_PROFILE_FLAG)

release: ## Build favai-cli (release)
	$(CARGO) build -p favai-cli --release

test: ## Run the workspace tests
	$(CARGO) test --workspace

start: build ## Start favai as a detached daemon
	$(FAVAI) --config $(CONFIG) start

stop: ## Stop the running daemon (no-op if not running)
	-@$(FAVAI) stop 2>/dev/null || true

kill: ## Hard-kill any favai process (ignores pid file)
	@pids=$$(pgrep -f '(^|/)favai($$| )' | grep -vx $$$$ || true); \
	if [ -z "$$pids" ]; then \
	  echo "no favai processes running"; \
	  rm -f $(HOME)/.config/starter/favai/favai.pid 2>/dev/null || true; \
	  exit 0; \
	fi; \
	echo "found favai processes:"; \
	ps -o pid,ppid,cmd -p $$pids; \
	echo "sending SIGTERM..."; \
	kill -TERM $$pids 2>/dev/null || true; \
	sleep 1; \
	remaining=$$(pgrep -f '(^|/)favai($$| )' | grep -vx $$$$ || true); \
	if [ -n "$$remaining" ]; then \
	  echo "sending SIGKILL to: $$remaining"; \
	  kill -KILL $$remaining 2>/dev/null || true; \
	fi; \
	rm -f $(HOME)/.config/starter/favai/favai.pid 2>/dev/null || true; \
	echo "done"

status: ## Report daemon liveness
	@$(FAVAI) status || true

restart: stop build start ## Stop -> build -> start

# `dev` keeps the console attached. We stop any existing daemon
# (so two agents don't fight over the same pid file), rebuild, and
# then run `daemon-run` in the foreground. Logs stream to this
# terminal; Ctrl-C delivers SIGINT which the daemon-run handler
# catches and shuts the agent down cleanly.
dev: stop build ## Build, stop any running daemon, run in foreground
	@echo "favai: running in foreground (Ctrl-C to exit)"
	exec $(FAVAI) --config $(CONFIG) daemon-run

logs: ## tail -f the daemon log
	@touch $(LOG_FILE)
	tail -f $(LOG_FILE)

doctor: build ## Show MCP host wiring
	$(FAVAI) doctor

demo: ## Run the bundled demo
	bash demo/run-demo.sh

# --- install / install-clean ------------------------------------------------
#
# Strategy: build a release binary, drop it in $(BINDIR), generate a
# systemd --user unit that runs `favai daemon-run` (foreground; systemd
# owns the process lifecycle, so we do NOT use `favai start`'s pid
# file), then enable + start it.
#
# We deliberately do not touch the host MCP configs here — use
# `favai doctor install <host>` for that. The two concerns are separate:
# `make install` keeps the periodic-sync daemon alive; `favai doctor`
# wires hosts to spawn `favai serve` themselves.

install: release ## Install binary + enable systemd --user service
	@command -v systemctl >/dev/null 2>&1 || { \
	  echo "make install: systemctl not found (this target requires systemd --user)"; \
	  exit 1; }
	install -d $(BINDIR)
	install -m 0755 $(CURDIR)/target/release/favai $(INSTALL_BIN)
	install -d $(SYSTEMD_DIR)
	install -d $(dir $(CONFIG))
	@if [ ! -f "$(CONFIG)" ]; then \
	  echo "make install: NOTE — no config at $(CONFIG); the service will"; \
	  echo "             start but exit immediately. Create one first."; \
	fi
	@printf '%s\n' \
	  '[Unit]' \
	  'Description=favai — MCP skill-favourites sync daemon' \
	  'Documentation=https://github.com/NubeDev/favai' \
	  'After=network-online.target' \
	  'Wants=network-online.target' \
	  '' \
	  '[Service]' \
	  'Type=simple' \
	  'ExecStart=$(INSTALL_BIN) --config $(CONFIG) daemon-run' \
	  'Restart=on-failure' \
	  'RestartSec=5s' \
	  'Environment=RUST_LOG=info' \
	  '# Logs go to the journal: journalctl --user -u $(UNIT_NAME) -f' \
	  '' \
	  '[Install]' \
	  'WantedBy=default.target' \
	  > $(UNIT_PATH)
	systemctl --user daemon-reload
	systemctl --user enable --now $(UNIT_NAME)
	@echo
	@echo "installed favai to $(INSTALL_BIN)"
	@echo "systemd unit:       $(UNIT_PATH)"
	@echo
	@systemctl --user --no-pager status $(UNIT_NAME) | head -10 || true
	@echo
	@echo "Tail logs with:  journalctl --user -u $(UNIT_NAME) -f"
	@echo "To survive logouts on a headless box, run once as root:"
	@echo "    loginctl enable-linger $$USER"

install-clean: ## Stop & disable the service, remove unit + binary
	@command -v systemctl >/dev/null 2>&1 || { \
	  echo "make install-clean: systemctl not found"; exit 1; }
	-systemctl --user disable --now $(UNIT_NAME) 2>/dev/null
	-rm -f $(UNIT_PATH)
	-systemctl --user daemon-reload
	-rm -f $(INSTALL_BIN)
	@echo "removed $(INSTALL_BIN) and $(UNIT_PATH)"
	@echo "(config + approvals at $(dir $(CONFIG)) were left in place)"

clean: ## cargo clean
	$(CARGO) clean
