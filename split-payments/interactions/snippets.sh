USER_PEM="~/Development/Work/Webit/bucharest-hackathon/sc-elrond/wallets/wallet-owner.pem"
PROXY="https://devnet-gateway.multiversx.com"
CHAIN_ID="D"

deploy() {
    mxpy --verbose contract deploy --project=${PROJECT} \
    --recall-nonce --pem=${USER_PEM} \
    --gas-limit=50000000 \
    --send --outfile="deploy-devnet.interaction.json" \
    --proxy=${PROXY} --chain=${CHAIN_ID} || return
}

upgradeSC() {
    mxpy --verbose contract upgrade ${CONTRACT_ADDRESS} --recall-nonce --payable \
        --bytecode=${WASM_PATH} \
        --pem=${WALLET_PEM} \
        --gas-limit=60000000 \
        --proxy=${PROXY} --chain=${CHAIN_ID} \
        --arguments $1 $2 \
        --send || return
}