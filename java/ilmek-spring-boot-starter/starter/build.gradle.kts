plugins {
    `java-library`
}

val bootVersion = providers.gradleProperty("bootVersion").getOrElse("4.1.2")
val bootLine = if (bootVersion.startsWith("3.")) "boot3" else "boot4"

group = "dev.ilmek"
version = "0.1.0-SNAPSHOT"

tasks.withType<JavaCompile>().configureEach {
    options.release.set(17)
    options.encoding = "UTF-8"
}

repositories {
    mavenCentral()
}

dependencies {
    api(platform("org.springframework.boot:spring-boot-dependencies:$bootVersion"))
    api("org.springframework:spring-context")
    api("org.springframework.boot:spring-boot-autoconfigure")
    implementation(platform("com.fasterxml.jackson:jackson-bom:2.22.4"))
    implementation("com.fasterxml.jackson.core:jackson-databind")
    implementation("org.springframework.boot:spring-boot-starter-data-jpa")
    testImplementation("org.junit.jupiter:junit-jupiter")
    testImplementation("org.springframework.boot:spring-boot-starter-test")
    if (bootLine == "boot4") {
        testImplementation("org.springframework.boot:spring-boot-starter-data-jpa-test")
    }
    testRuntimeOnly("com.h2database:h2")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

sourceSets.test {
    java.srcDir("src/test-$bootLine/java")
}

tasks.test {
    useJUnitPlatform()
}
