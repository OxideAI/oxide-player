# Test harness for the oxide-player installer (install.sh).
#
# install.sh is a Debian/Ubuntu systemd-based installer that ends with
# `systemctl enable/restart`. To exercise it faithfully we need a container
# running systemd as PID 1.
#
# Build:
#   docker build -t oxide-install-test -f contrib/docker/Dockerfile .
#
# Run (privileged is required for systemd):
#   docker run -d --privileged --name oxide-test oxide-install-test
#
# Then drop into it and run the installer, e.g.:
#   docker exec -it oxide-test bash -c \
#     'BRANCH=feat/oxide-player-v1 bash <(curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/feat/oxide-player-v1/install.sh)'

FROM debian:bookworm

ENV DEBIAN_FRONTEND=noninteractive

# systemd as init + CA certs (needed for the HTTPS git clone the installer does)
# + build tools needed by install.sh: git, curl, nodejs (+npm) for the frontend
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      systemd systemd-sysv \
      git curl ca-certificates \
      nodejs npm \
 && rm -rf /var/lib/apt/lists/*

# Let systemd run as the container init. Remove the getty/udev autostart
# units that fight the container environment.
RUN find /etc/systemd/system/*.wants -type l -delete 2>/dev/null || true
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/*.wants/* 2>/dev/null || true

STOPSIGNAL SIGRTMIN+3
CMD ["/lib/systemd/systemd"]
