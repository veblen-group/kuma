# kuma
![kuma](kuma.png)

# todo
- [ ] collect binance data to stream
- [ ] collect uniswap data to stream
- [ ] save binance data to db
- [ ] save uniswap data to db
- [ ] detect arbs
- [ ] set up alerts for arbs
- [ ] execute both directions of an arb


# resources
## bot architecture
[excalidraw link](https://excalidraw.com/#room=ca67da2b51930cc17ca7,lfzj6eSMJd40Hcf9WTaJpg)
hummingbot

- [hummingbot exchange connectors](https://github.com/hummingbot/hummingbot/tree/master/hummingbot/connector/exchange)
    - [binance](https://github.com/hummingbot/hummingbot/blob/master/hummingbot/connector/exchange/binance)
    - they also have perps
- [hummingbot gateway connectors](https://github.com/hummingbot/gateway/tree/main/src/connectors)
    - [uniswap](https://github.com/hummingbot/gateway/tree/main/src/connectors/uniswap)

[penumbra-zone/osiris](https://github.com/penumbra-zone/osiris/tree/main)

## ethereum
[geth json rpc docs](https://geth.ethereum.org/docs/interacting-with-geth/rpc)

### uniswap
[univ3 sdk](https://docs.uniswap.org/sdk/v3/overview)
[univ3 math](https://www.desmos.com/calculator/q2kxfue441)
[univ3 math rs](https://github.com/0xKitsune/uniswap-v3-math)
[`uniswap-v3-sdk-rs`](https://github.com/shuhuiluo/uniswap-v3-sdk-rs)

## binance
[binance api docs](https://developers.binance.com/docs/binance-spot-api-docs/web-socket-streams)
[`binance-spot-connector-rust`](https://github.com/binance/binance-spot-connector-rust/tree/main)

`binance-rs`:
- [docs](https://docs.rs/binance/0.21.0/binance/index.html)
- [repo](https://github.com/wisespace-io/binance-rs/tree/master)
