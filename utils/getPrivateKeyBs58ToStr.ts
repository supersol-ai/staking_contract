const { bs58 } = require("@coral-xyz/anchor/dist/cjs/utils/bytes");

const privateKey58 = []; // paste privateKey in base58 format
const strPrivateKey = bs58.encode(privateKey58);
console.log(strPrivateKey)