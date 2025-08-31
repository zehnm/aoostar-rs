FROM rust:1-bookworm AS build-env
WORKDIR /app
COPY . /app
RUN cd /tmp && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
        libudev-dev \
        && \
    apt-get download \
        libudev1 \
        libc6 \
        libcap2 \
        && \
    mkdir -p /dpkg/var/lib/dpkg/status.d/ && \
    for deb in *.deb; do \
        package_name=$(dpkg-deb -I ${deb} | awk '/^ Package: .*$/ {print $2}'); \
        echo "Process: ${package_name}"; \
        dpkg --ctrl-tarfile $deb | tar -Oxf - ./control > /dpkg/var/lib/dpkg/status.d/${package_name}; \
        dpkg --extract $deb /dpkg || exit 10; \
    done
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY /cfg /cfg
COPY /fonts /fonts
COPY /linux/scripts/cpu_usage.sh /linux/scripts/mem_usage.sh /
COPY --from=build-env /dpkg/ /
COPY --from=build-env /app/target/release/asterctl /
COPY --from=build-env /app/target/release/aster-sysinfo /
CMD ["./asterctl"]
