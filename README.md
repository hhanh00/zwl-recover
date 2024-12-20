## Recovery tool for Zecwallet Lite

Use this if you have a wallet you can no longer access
because the app does not start but know the seed phrase.

```
zwl-recover.AppImage --ntaddrs <NTADDRS> --nzaddrs <NZADDRS> --birth-height <BIRTH_HEIGHT> --seed <SEED> --destination <DESTINATION> --lwd-url <LWD_URL>
```

- `seed`: seed phrase (24 words)
- `destination`: address where you want the funds sent
- `birth-height`: height at which the wallet was created
- `ntaddrs`: number of transparent addresses to scan
- `nzaddrs`: number of sapling addresses to scan
- `lwd_url`: URL of a lightwalletd server (ex: https://zec.rocks)
