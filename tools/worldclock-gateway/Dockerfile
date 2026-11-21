# The world-time gateway on Debian 12 slim (glibc 2.36). build-image.sh puts the release binary in the context.
FROM debian:bookworm-slim
COPY worldclock-gateway /usr/local/bin/worldclock-gateway
USER 65534:65534
EXPOSE 4318 4319
ENTRYPOINT ["/usr/local/bin/worldclock-gateway"]
