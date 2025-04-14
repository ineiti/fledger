from mergexp import *

net = Network('exp11', routing == static)

def makeNode(i: int):
    name = f"n{i}"
    return net.node(name, proc.cores>=1, memory.capacity>=mb(512))

lans = [
    [makeNode(i) for i in range(25)],
    [makeNode(i) for i in range(25, 50)],
    [makeNode(i) for i in range(50, 75)],
    [makeNode(i) for i in range(75, 100)]
]

router01 = net.node('router01', proc.cores>=1, memory.capacity>=mb(512))
router12 = net.node('router12', proc.cores>=1, memory.capacity>=mb(512))
router23 = net.node('router23', proc.cores>=1, memory.capacity>=mb(512))

central = net.node('central', proc.cores>=2, memory.capacity>=mb(512))

lans[0].extend([router01])
lans[1].extend([router01, router12])
lans[2].extend([router12, router23])
lans[3].extend([router23])

links = [
    #net.connect(lans[i], capacity==mbps(100), latency==ms(10)) 
    net.connect(lans[i]) # atm capacity and latency break sphere 
    for i in range(len(lans))
]


links[0][router01].socket.addrs = ip4(f"10.0.0.2/24")

links[1][router01].socket.addrs = ip4(f"10.0.1.1/24")
links[1][router12].socket.addrs = ip4(f"10.0.1.2/24")

links[2][router12].socket.addrs = ip4(f"10.0.2.1/24")
links[2][router23].socket.addrs = ip4(f"10.0.2.2/24")

links[3][router23].socket.addrs = ip4(f"10.0.3.1/24")

centralLan = [router12, central]
#centralLink = net.connect(centralLan, capacity==mbps(100), latency==ms(10))
centralLink = net.connect(centralLan)

centralLink[central].socket.addrs = ip4(f"10.0.128.128/24")
centralLink[router12].socket.addrs = ip4(f"10.0.128.1/24")

for i in range(len(links)):
    lan = lans[i]
    link = links[i]
    for j in range(25):
        n = str(j + 10)
        link[lan[j]].socket.addrs = ip4(f"10.0.{i}.{n}/24")

experiment(net)
