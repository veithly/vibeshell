FROM docker:29-dind
RUN apk add --no-cache openssh-server python3 bash \
    && adduser -D -s /bin/sh audit && echo 'audit:isolated-container-test-only' | chpasswd \
    && addgroup audit docker && mkdir -p /run/sshd
COPY start.sh /fixture-start.sh
EXPOSE 22
ENTRYPOINT ["/bin/sh", "/fixture-start.sh"]
