from mergexp import *

net = Network('exp3', routing == static)

def makeNode(i: int):
    name = f"n{i}"
    return net.node(name, proc.cores>=1, memory.capacity>=mb(512))

sna = [makeNode(i) for i in range(5)]
snb = [makeNode(i) for i in range(5, 10)]

router = net.node('router', proc.cores>=1, memory.capacity>=mb(512))
signaling = net.node('signaling', proc.cores>=2, memory.capacity>=mb(512))

sna.extend([router, signaling])
snb.extend([router, signaling])

linka = net.connect(sna)
linkb = net.connect(snb)

linka[router].socket.addrs = ip4("10.0.0.1/24")
linkb[router].socket.addrs = ip4("10.0.1.1/24")

linka[signaling].socket.addrs = ip4("10.0.0.128/24")
linkb[signaling].socket.addrs = ip4("10.0.1.128/24")

    
for i in range(5):
    suffix = str(i + 2)
    linka[sna[i]].socket.addrs = ip4(f"10.0.0.{suffix}/24")
    linkb[snb[i]].socket.addrs = ip4(f"10.0.1.{suffix}/24")


experiment(net)
