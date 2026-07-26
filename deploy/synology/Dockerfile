ARG BASE_IMAGE=ghcr.io/uniclipboard/uniclipboard-server:0.20.0-alpha.2
FROM ${BASE_IMAGE}

USER root
RUN apk add --no-cache nodejs
COPY deploy/synology/admin-web/ /opt/uniclipboard-admin-web/
COPY deploy/synology/uniclipboard-server-entrypoint.sh /usr/local/bin/uniclipboard-server-entrypoint
RUN chmod +x /usr/local/bin/uniclipboard-server-entrypoint \
 && chmod +x /opt/uniclipboard-admin-web/server.js \
 && ln -s /opt/uniclipboard-admin-web/server.js /usr/local/bin/uniclipboard-admin-web \
 && chown -R uniclip:uniclip /usr/local/bin/uniclipboard-server-entrypoint /opt/uniclipboard-admin-web

USER uniclip
CMD ["uniclipboard-server-entrypoint"]
