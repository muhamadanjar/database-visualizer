#!/bin/sh
set -eu
test -f /.dockerenv || { echo "Run this gateway only in its Docker network namespace" >&2; exit 1; }
# Namespace owner installs kernel rules; API has no NET_ADMIN capability.
iptables -P OUTPUT DROP
ip6tables -P OUTPUT DROP
iptables -F OUTPUT
ip6tables -F OUTPUT
for firewall in iptables ip6tables; do
  "$firewall" -A OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
done
iptables -A OUTPUT -d 127.0.0.11 -p udp --dport 53 -j ACCEPT
iptables -A OUTPUT -d 127.0.0.11 -p tcp --dport 53 -j ACCEPT
iptables -A OUTPUT -d 127.0.0.1 -p tcp --dport 8080 -j ACCEPT
iptables -A OUTPUT -d 172.30.1.2 -p tcp --dport 5432 -j ACCEPT
# Metadata endpoints are denied even when accidentally added to target policy.
for network in 169.254.0.0/16 100.100.100.200/32 224.0.0.0/4 0.0.0.0/32; do
  iptables -A OUTPUT -d "$network" -j REJECT
done
for network in fe80::/10 fd00:ec2::254/128 ff00::/8 ::/128; do
  ip6tables -A OUTPUT -d "$network" -j REJECT
done
jq -er 'length > 0 and length <= 256 and all(.[]; (.port|type == "number") and (.port >= 1 and .port <= 65535) and (.network|test("^[0-9a-fA-F.:]+/[0-9]{1,3}$")))' /policy/targets.json >/dev/null
# Kernel rules use operator CIDRs; application validates exact host and every DNS answer.
jq -r '.[] | [.network, (.port|tostring)] | @tsv' /policy/targets.json > /tmp/targets
while read -r target port; do
  case "$target" in *:*) firewall=ip6tables ;; *) firewall=iptables ;; esac
  "$firewall" -A OUTPUT -d "$target" -p tcp --dport "$port" -j ACCEPT
done < /tmp/targets
# Fixed trusted OAuth HTTPS endpoints use Docker DNS. Private networks are excluded;
# explicit customer/control-plane tuples above are the exceptions.
for network in 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16 127.0.0.0/8 100.64.0.0/10; do
  iptables -A OUTPUT -d "$network" -j REJECT
done
ip6tables -A OUTPUT -d fc00::/7 -j REJECT
iptables -A OUTPUT -p tcp --dport 443 -j ACCEPT
ip6tables -A OUTPUT -p tcp --dport 443 -j ACCEPT
touch /tmp/ready
exec tail -f /dev/null
