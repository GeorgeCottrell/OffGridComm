# Build stage
FROM rust:1.60 AS builder
WORKDIR /usr/src/offgridcomm
COPY . .
RUN cargo build --release

# Runtime stage
FROM ubuntu:20.04
# Install dependencies
RUN apt-get update && apt-get install -y \
    supervisor \
    openssh-server \
    sqlite3 \
    && rm -rf /var/lib/apt/lists/*
# Copy the built binary
COPY --from=builder /usr/src/offgridcomm/target/release/offgridcomm /usr/local/bin/offgridcomm
# Set up SSH
RUN mkdir /var/run/sshd \
    && echo 'root:offgrid' | chpasswd \
    && sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config
# Create data directory
RUN mkdir -p /app/data
# Copy supervisord configuration
COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf
# Expose ports
EXPOSE 8080 22
# Start supervisord
CMD ["/usr/bin/supervisord"]
