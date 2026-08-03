# Rspamd controller (local.d/worker-controller.inc).
#
# The controller port (11334) is reachable ONLY on the private compose
# network — it is never published to the host or the internet (see
# docker-compose.yml: no ports mapping). `secure_ip` formalizes that
# trust: peers on the RFC 1918 docker ranges (our own services) may
# call privileged endpoints — which is what lets alo-jmap POST
# /learnspam and /learnham for junk training without a shared secret
# to rotate. Anything that can reach this port at all is already
# inside the deployment's trust boundary.
secure_ip = "127.0.0.1";
secure_ip = "::1";
secure_ip = "172.16.0.0/12";
