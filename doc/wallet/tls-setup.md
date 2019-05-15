# Wallet TLS setup

## What you need
* A server with a static IP address (eg `3.3.3.3`)
* A domain name ownership (`example.com`)
* DNS configuration for this IP (`epic1.example.com` -> `3.3.3.3`)

If you don't have a static IP you may want to consider using services like DynDNS which support dynamic IP resolving, this case is not covered by this guide, but all the next steps are equally applicable.

If you don't have a domain name there is a possibility to get a TLS certificate for your IP, but you have to pay for that (so perhaps it's cheaper to buy a domain name) and it's rarely supported by certificate providers.

## I have a TLS certificate already
Uncomment and update the following lines in wallet config (by default `~/.epic/epic-wallet.toml`):

```toml
tls_certificate_file = "/path/to/my/cerificate/fullchain.pem"
tls_certificate_key =  "/path/to/my/cerificate/privkey.pem"
```

And update `api_listen_interface` to your static IP if you want to lock your wallet only to external interface

```toml
api_listen_interface = "3.3.3.3"
```

Or, in case you are using DynDNS or `localhost` in order to comunicate with your wallet, just put `0.0.0.0` as mentioned in the inline instruction.

```toml
api_listen_interface = "0.0.0.0"
```

If you have Stratum server enabled (you run a miner) make sure that wallet listener URL starts with `https` in node config (by default `~/.epic/epic-server.toml`):

```toml
wallet_listener_url = "https://epic1.example.com:13415"
```

Make sure your user has read access to the files (see below for how to do it). Restart wallet. If you changed your node configuration restart `epic` too. When you (or someone else) send epics to this wallet the destination (`-d` option) must start with `https://`, not with `http://`.

## I don't have a TLS certificate
You can get it for free from [Let's Encrypt](https://letsencrypt.org/). To simplify the process we need `certbot`.

### Install certbot
Go to [Certbot home page](https://certbot.eff.org/), choose I'm using `None of the above` and your OS (eg `Ubuntu 18.04` which will be used as an example). You will be redirected to a page with instructions like [steps for Ubuntu](https://certbot.eff.org/lets-encrypt/ubuntubionic-other). Follow instructions from `Install` section. As result you should have `certbot` installed.

### Obtain certificate
If you have experince with `certboot` feel free to use any type of challenge. This guide covers the simplest case of HTTP challenge. For this you need to have a web server listening on port `80`, which requires running it as root in the simplest case. We will use the server provided by certbot. **Make sure you have port 80 open**

```sh
sudo certbot certonly --standalone -d epic1.example.com
```

It will ask you some questions, as result you should see something like:

```
Congratulations! Your certificate and chain have been saved at:
  /etc/letsencrypt/live/epic1.example.com/fullchain.pem
 Your key file has been saved at:
  /etc/letsencrypt/live/epic1.example.com/privkey.pem
 Your cert will expire on 2019-01-16. To obtain a new or tweaked
 version of this certificate in the future, simply run certbot
 again. To non-interactively renew *all* of your certificates, run
"certbot renew"
```

### Change permissions
Now you have the certificate files but only root user can read it. We run epic as `ubuntu` user. There are different scenarios how to fix it, the simplest one is to create a group which will have access to `/etc/letsencrypt` directory and add our user to this group.

```sh
sudo groupadd tls-cert
sudo usermod -a -G tls-cert ubuntu
sudo chgrp -R tls-cert /etc/letsencrypt
sudo chmod -R g=rX /etc/letsencrypt
sudo chmod 2755 /etc/letsencrypt
```

The last step is needed for renewal, it makes sure that all new files will have the same group ownership.

Now you need to logout so the user's group membership modification can take place.

### Update wallet config
Refer to `I have a TLS certificate already` because you have it now. Use the folowing values:

```toml
tls_certificate_file = "/etc/letsencrypt/live/epic1.example.com/fullchain.pem"
tls_certificate_key =  "/etc/letsencrypt/live/epic1.example.com/privkey.pem"
```

