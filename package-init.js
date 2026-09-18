import fs from "fs/promises";

/**********
 * ASSETS *
 **********/

await fs.copyFile("assets/icon.ico", "src-tauri/icons/icon.ico");
await fs.copyFile("assets/icon.icns", "src-tauri/icons/icon.icns");

/***********
 * ANDROID *
 ***********/

try {
  await fs.stat("./src-tauri/gen/android");
} catch (error) {
  process.exit();
}

try {
  await fs.stat("./src-tauri/gen/android/keystore.properties");
  process.exit();
} catch (error) {}

let target = `
  password=${process.env.ANDROID_CERTIFICATE_PASSWORD}
  keyAlias=upload
  storeFile=${process.env.HOME}/upload-keystore.jks
`;

await fs.writeFile("./src-tauri/gen/android/keystore.properties", target);

target = await fs.readFile(
  "./src-tauri/gen/android/app/build.gradle.kts",
  "utf-8",
);

target = target.replace(
  "import java.util.Properties",
  "import java.util.Properties\nimport java.io.FileInputStream",
);

target = target.replace(
  "buildTypes",
  `signingConfigs {
    create("release") {
      val keystorePropertiesFile = rootProject.file("keystore.properties")
      val keystoreProperties = Properties()
      if (keystorePropertiesFile.exists()) {
          keystoreProperties.load(FileInputStream(keystorePropertiesFile))
      }
      keyAlias = keystoreProperties["keyAlias"] as String
      keyPassword = keystoreProperties["password"] as String
      storeFile = file(keystoreProperties["storeFile"] as String)
      storePassword = keystoreProperties["password"] as String
    }
  }
  buildTypes`,
);

target = target.replace(
  /(?<=getByName\("release"\)\s*){/s,
  '{\nsigningConfig = signingConfigs.getByName("release")',
);

await fs.writeFile("./src-tauri/gen/android/app/build.gradle.kts", target);

target = await fs.readFile(
  "./src-tauri/gen/android/app/src/main/res/xml/file_paths.xml",
  "utf-8",
);

target = target.replace(/path="[^"].*?"/gs, 'path="../"');

await fs.writeFile(
  "./src-tauri/gen/android/app/src/main/res/xml/file_paths.xml",
  target,
);
