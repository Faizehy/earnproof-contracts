import { createHash } from "node:crypto";

const sha256Hex = (value: string) => createHash("sha256").update(Buffer.from(value, "utf8")).digest("hex");
console.log(sha256Hex("proof:example:1"));
const schema = Buffer.alloc(4);
schema.writeUInt32BE(7);
console.log(schema.toString("hex"));
const expiration = Buffer.alloc(8);
expiration.writeBigUInt64BE(1700000000n);
console.log(expiration.toString("hex"));

const domainHash = (...parts: Buffer[]) => createHash("sha256").update(Buffer.concat(parts)).digest();
const ascii = (value: string) => Buffer.from(value, "ascii");
const networkPassphrase = "Test SDF Network ; September 2015";
const issuer = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";
const otherIssuer = "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG";
const claim = Buffer.from("7261c38367d18cd03b133d7011956d1a8a35daf3e379aed2d45cdf33be235f35", "hex");
const claimId = Buffer.from("c5aecb1a93a48d868c6708d746a71d7eb57f0cfd7a18f0659f97d34fc63efa19", "hex");
const network = domainHash(ascii("earnproof.network.v1\0"), ascii(networkPassphrase));
const nativeAsset = domainHash(ascii("earnproof.asset.v1\0"), ascii("native"));
const code = ascii("USDC");
const issuedAsset = domainHash(
	ascii("earnproof.asset.v1\0"),
	ascii("issued\0"),
	Buffer.from([code.length]),
	code,
	ascii(issuer),
);
const proofContext = (asset: Buffer) => domainHash(
	ascii("earnproof.proof-context.v1\0"),
	claim,
	network,
	asset,
);
const proofRecordId = (context: Buffer) => domainHash(
	ascii("earnproof.proof-record.v1\0"),
	claimId,
	context,
);

console.log(JSON.stringify({
	network: network.toString("hex"),
	native: {
		asset: nativeAsset.toString("hex"),
		context: proofContext(nativeAsset).toString("hex"),
		recordId: proofRecordId(proofContext(nativeAsset)).toString("hex"),
	},
	issuedUSDC: {
		asset: issuedAsset.toString("hex"),
		context: proofContext(issuedAsset).toString("hex"),
		recordId: proofRecordId(proofContext(issuedAsset)).toString("hex"),
	},
}, null, 2));

const pseudonymCommitment = (domain: string, issuerAddress: string, pseudonym: Buffer) => {
	const domainBytes = ascii(domain);
	if (domainBytes.length === 0 || domainBytes.length > 64 || /[^\x21-\x7e]/.test(domain)) {
		throw new Error("Invalid subject pseudonym domain");
	}
	return domainHash(
		ascii("earnproof.subject-pseudonym.v1\0"),
		Buffer.from([domainBytes.length]),
		domainBytes,
		ascii(issuerAddress),
		pseudonym,
	);
};
const subjectPseudonym = Buffer.alloc(32, 0x11);
console.log(JSON.stringify({
	credentialVerification: pseudonymCommitment(
		"credential-verification",
		issuer,
		subjectPseudonym,
	).toString("hex"),
	analyticsDomain: pseudonymCommitment("analytics", issuer, subjectPseudonym).toString("hex"),
	otherIssuer: pseudonymCommitment(
		"credential-verification",
		otherIssuer,
		subjectPseudonym,
	).toString("hex"),
}, null, 2));