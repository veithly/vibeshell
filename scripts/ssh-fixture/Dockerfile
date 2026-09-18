FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends openssh-server python3 ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd -m -s /bin/sh audit && echo 'audit:ponytail-isolated-test-only' | chpasswd \
    && mkdir -p /run/sshd /home/audit/.ssh && chmod 700 /home/audit/.ssh && chown -R audit:audit /home/audit/.ssh
COPY sshd_config /etc/ssh/sshd_config
COPY start.sh /start.sh
EXPOSE 22 23 24
CMD ["/bin/sh", "/start.sh"]
