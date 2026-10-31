import org.springframework.boot.gradle.plugin.SpringBootPlugin

plugins {
    java
    id("org.springframework.boot")
}

group = "dev.ilmek.examples"
version = "0.1.0-SNAPSHOT"

val bootVersion = providers.gradleProperty("bootVersion").getOrElse("4.1.2")
val bootLine = if (bootVersion.startsWith("3.")) "boot3" else "boot4"

repositories {
    mavenCentral()
}

dependencies {
    implementation(platform(SpringBootPlugin.BOM_COORDINATES))
    implementation(platform("com.fasterxml.jackson:jackson-bom:2.22.4"))
    implementation("dev.ilmek:starter")
    implementation("org.springframework.boot:spring-boot-starter-data-jpa")
    implementation("com.fasterxml.jackson.core:jackson-databind")
    runtimeOnly("com.h2database:h2")
    testImplementation("org.springframework.boot:spring-boot-starter-test")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<JavaCompile>().configureEach {
    options.release.set(17)
    options.encoding = "UTF-8"
}

tasks.test {
    useJUnitPlatform()
}

tasks.bootJar {
    archiveFileName.set("backend-java.jar")
}
