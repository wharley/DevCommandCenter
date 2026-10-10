const fs = require("node:fs");
const path = require("node:path");

// Expo loads app.json and hands it here as `config`.
//
// google-services.json (Firebase, needed for Android push) stays out of this
// public repository: EAS builds receive it through the GOOGLE_SERVICES_JSON
// file variable, local builds read apps/mobile/google-services.json when it
// exists. Without either, the Android app builds without push.
module.exports = ({ config }) => {
	const local = path.join(__dirname, "google-services.json");
	const googleServicesFile =
		process.env.GOOGLE_SERVICES_JSON ?? (fs.existsSync(local) ? "./google-services.json" : undefined);
	if (!googleServicesFile) return config;
	return { ...config, android: { ...config.android, googleServicesFile } };
};
