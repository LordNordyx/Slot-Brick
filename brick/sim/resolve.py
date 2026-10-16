"""The address of a host the sim must reach, for machines whose resolver cannot say.

Asks a public resolver over plain DNS, and falls back to this machine's own. That way round
because the hosts asked for are public ones, and because a resolver that cannot answer for a
host (one that does not handle the .fyi the label server lives on, say) takes its time saying
so, which would hold up every start of the sim. Prints one dotted quad, or nothing at all.
Only ever used to tell the sim's container an address for a host it would otherwise fail to
look up.
"""

import socket
import struct
import sys

# Asked only when this machine's own resolver has already failed.
PUBLIC = ("1.1.1.1", "8.8.8.8")
TIMEOUT = 3.0


def system(host):
    try:
        return socket.gethostbyname(host)
    except OSError:
        return None


def query(server, host):
    """One A query, and the first address its answer holds."""
    head = struct.pack(">HHHHHH", 0x5107, 0x0100, 1, 0, 0, 0)
    name = b"".join(bytes([len(p)]) + p for p in host.encode("idna").split(b".")) + b"\0"
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(TIMEOUT)
    try:
        sock.sendto(head + name + struct.pack(">HH", 1, 1), (server, 53))
        reply = sock.recv(4096)
    except OSError:
        return None
    finally:
        sock.close()
    if len(reply) < 12 or reply[:2] != head[:2]:
        return None
    at = 12
    # Past the question, then through the answers to the first A record.
    for _ in range(struct.unpack(">H", reply[4:6])[0]):
        at = skip(reply, at) + 4
    for _ in range(struct.unpack(">H", reply[6:8])[0]):
        at = skip(reply, at)
        if at + 10 > len(reply):
            return None
        kind, length = struct.unpack(">HH", reply[at : at + 2] + reply[at + 8 : at + 10])
        at += 10
        if kind == 1 and length == 4 and at + 4 <= len(reply):
            return ".".join(str(b) for b in reply[at : at + 4])
        at += length
    return None


def skip(reply, at):
    """Past a name, which is labels until a zero or a compression pointer."""
    while at < len(reply):
        length = reply[at]
        if length == 0:
            return at + 1
        if length >= 0xC0:
            return at + 2
        at += length + 1
    return at


def main(hosts):
    for host in hosts:
        addr = None
        for server in PUBLIC:
            addr = addr or query(server, host)
        if not addr:
            addr = system(host)
        if addr:
            print(addr)
            return 0
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
