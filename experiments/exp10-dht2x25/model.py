from mergexp import *

net = Network('exp10', routing == static)

def makeNode(i: int):
    name = f"n{i}"
    return net.node(name, proc.cores>=1, memory.capacity>=mb(512))

sna = [makeNode(i) for i in range(25)]
snb = [makeNode(i) for i in range(25, 50)]

router = net.node('router', proc.cores>=1, memory.capacity>=mb(512))
central = net.node('central', proc.cores>=2, memory.capacity>=mb(512))

sna.extend([router, central])
snb.extend([router, central])

linka = net.connect(sna, capacity==mbps(100), latency==ms(10))
linkb = net.connect(snb, capacity==mbps(100), latency==ms(10))

linka[router].socket.addrs = ip4("10.0.0.1/24")
linkb[router].socket.addrs = ip4("10.0.1.1/24")

linka[central].socket.addrs = ip4("10.0.0.128/24")
linkb[central].socket.addrs = ip4("10.0.1.128/24")

for i in range(25):
    suffix = str(i + 10)
    linka[sna[i]].socket.addrs = ip4(f"10.0.0.{suffix}/24")
    linkb[snb[i]].socket.addrs = ip4(f"10.0.1.{suffix}/24")

experiment(net)
