# DHT Storage for webpages

Using the DHT storage module from fledger, you can store data in a decentralized way.
A first use-case is to store and retrieve webpages from fledger.
Here is how it works:

1. You create a static version of your website (with [Hugo](https://gohugo.io/), [Jekyll](https://jekyllrb.com/) or others)
2. You upload it to the fledger network
3. Your webpage is served from your node while its online, or from other people's node when you're offline

In a future version, fledger will also track how long you're online and how many other pages
you're serving.
This will allow a tit-for-tat sharing: the longer and the more other pages you store and serve,
the longer other nodes wil store and serve your pages.

# Exploring existing domains

To upload your web page to fledger, you can simply go to the [Fledger Web](https://web.fledg.re) to connect
as a node.
Choose the _Manage Pages_ tab.
Fledger will automatically choose the standard root-domain for you.
You will see all subdomains of the root-domain available and can click
on any of them for sub-sub domains or the homepage.

## Adding your own homepage

To add your own homepage, you need to create a domain for it.
Then you can upload files and directories which will be stored
under this domain.
Per default, `index.html` or `index.md` is used as the homepage, but
you can also change this.

### Creating your domain

Each domain can decide whether it wants to accept random sub-domains or
whether it restricts them to the owner.
The root-domain accepts any sub-domain, and is selected by default.
Follow the steps to create your sob-domain and upload your page:

1. Click `Add subdomain`
2. Enter the desired name - fledger will tell you if the name is available or not
3. Confirm with `Add`

Please be aware that fledger will create a private key for protecting access to
your domain.
If you lose this key, you won't be able to modify the domain anymore.

### Uploading your files



## Private key storage

The private keys created to protect your domain will be stored directly in your
browser.
So if you delete the `localStorage`, you will lose all your private keys and access
to your domain!
You can create a backup in the `Profile` tab, and store the json file somewhere safe on
your harddisk.
To restore a backup, you can upload it in the `Profile` tab.

## Technical details

Fledger does the following when you set up your own webpage:

1. Creates a `Signer` and stores it locally
1. Creates and stores an `Identity` as a `FloIdentity`
1. Creates an `ACE` with the rules `domain.*`, `blob.*` and stores it as a `FloACE`
1. Creates a `Domain` updatable by the `ACE`
1. Creates a `FloBlob` with content-type `text/html` and links it with the `Domain`

### Questions

- should the `Domain` point to a home-page?
- is there a need for a hierarchical storage of the FloBlobs?
- how to discover the Domain given the name? 
  - ID of the domain could depend on the hash of the name
  - a separate DNS system to resolve names to IDs
- how to discover the pages stored?
  - Create a list of all domains if they point to a home-page
  - the home-pages link to the other pages
  - use sub-domains for different topics in the domains of the users
- should there be a main-domain with given subdomains (social, blog, tests) that can
be populated by new users?
- how to bootstrap the system?
  - create one page for fledger, and hardcode this ID in the fledger-binary