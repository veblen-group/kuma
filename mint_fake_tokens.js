// fundWithWhales.js
const { ethers } = require("ethers");


// ETHER mainnet values

// const WETH = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"; // WETH
// const UNI = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984"; // UNI
// const WETH_WHALE = "0x8EB8a3b98659Cce290402893d0123abb75E3ab28";
// const UNI_WHALE = "0x5069A64BC6616dEC1584eE0500B7813A9B680F7E";


// UNICHAIN values

const WETH = "0x4200000000000000000000000000000000000006"; // WETH
const UNI = "0x8f187aa05619a017077f5308904739877ce9ea21"; // UNI
const WETH_WHALE = "0x07aE8551Be970cB1cCa11Dd7a11F47Ae82e70E67";
const UNI_WHALE = "0x2B38fEAdf26e54454FfBf3d8589b84D1E977954A";

// Your local Anvil test account (default Anvil #0)
const RECEIVER = "0xaC8C664c87B89df0e3d2A21Cb2815a72892cEfc3";

// Minimal ERC20 ABI
const ERC20_ABI = [
    "function balanceOf(address) view returns (uint256)",
    "function transfer(address to, uint256 value) returns (bool)",
    "function decimals() view returns (uint8)"
];

async function main() {
    const provider = new ethers.JsonRpcProvider("http://127.0.0.1:8546");

    // Attach contracts
    const weth = new ethers.Contract(WETH, ERC20_ABI, provider);
    const uni = new ethers.Contract(UNI, ERC20_ABI, provider);

    // --- WETH funding ---
    console.log("🔹 Impersonating WETH whale:", WETH_WHALE);
    await provider.send("anvil_impersonateAccount", [WETH_WHALE]);
    const wethWhaleSigner = await provider.getSigner(WETH_WHALE);

    const wethDecimals = await weth.decimals();
    const wethAmount = ethers.parseUnits("2", wethDecimals);

    await weth.connect(wethWhaleSigner).transfer(RECEIVER, wethAmount);
    console.log(`✅ Sent 2 WETH to ${RECEIVER}`);

    // --- UNI funding ---
    console.log("🔹 Impersonating UNI whale:", UNI_WHALE);
    await provider.send("anvil_impersonateAccount", [UNI_WHALE]);
    const uniWhaleSigner = await provider.getSigner(UNI_WHALE);

    const uniDecimals = await uni.decimals();
    const uniAmount = ethers.parseUnits("5000", uniDecimals);

    await uni.connect(uniWhaleSigner).transfer(RECEIVER, uniAmount);
    console.log(`✅ Sent 5000 UNI to ${RECEIVER}`);

    // Check balances
    const finalWeth = await weth.balanceOf(RECEIVER);
    const finalUni = await uni.balanceOf(RECEIVER);

    console.log("🎉 Final balances:");
    console.log("WETH:", ethers.formatUnits(finalWeth, wethDecimals));
    console.log("UNI :", ethers.formatUnits(finalUni, uniDecimals));
}

main().catch(err => {
    console.error("❌ Error:", err);
    process.exit(1);
});
