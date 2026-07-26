ARG BASE_IMAGE=ghcr.io/uniclipboard/uniclipboard-server:0.20.0-alpha.2
FROM ${BASE_IMAGE}

USER root
COPY deploy/synology/uniclipboard-server-entrypoint.sh /usr/local/bin/uniclipboard-server-entrypoint
RUN chmod +x /usr/local/bin/uniclipboard-server-entrypoint \
 && chown uniclip:uniclip /usr/local/bin/uniclipboard-server-entrypoint

USER uniclip
CMD ["uniclipboard-server-entrypoint"]
