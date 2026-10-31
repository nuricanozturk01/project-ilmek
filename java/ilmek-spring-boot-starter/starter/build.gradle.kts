plugins {
    `java-library`
}

group = "dev.ilmek"
version = "0.1.0-SNAPSHOT"

tasks.withType<JavaCompile>().configureEach {
    options.release.set(21)
    options.encoding = "UTF-8"
}

repositories {
    mavenCentral()
}

dependencies {
    implementation(platform("com.fasterxml.jackson:jackson-bom:2.22.4"))
    implementation("com.fasterxml.jackson.core:jackson-databind")
    testImplementation(platform("org.junit:junit-bom:5.11.3"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()
}
