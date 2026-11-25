# rspigot-probe


`rspigot-probe` is an offline-mode client that logs in like a 26.3 client and measures chunk delivery:

```
./target/release/rspigot-probe --address 127.0.0.1:25565 --view 10 --walk 640 --speed 4
```

It prints one JSON line: time to first chunk, time until the whole view is loaded, chunks per second, duplicates, and how many chunks near the player were missing while walking.
