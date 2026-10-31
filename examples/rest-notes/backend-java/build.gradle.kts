import org.springframework.boot.gradle.plugin.SpringBootPlugin

plugins {
    java
    id("org.springframework.boot")
}

group = "dev.ilmek.examples"
version = "0.1.0-SNAPSHOT"

val bootVersion = providers.gradleProperty("bootVersion").getOrElse("4.1.2")
val webStarter = if (bootVersion.startsWith("3.")) "spring-boot-starter-web" else "spring-boot-starter-webmvc"

repositories {
    mavenCentral()
}

dependencies {
    implementation(platform(SpringBootPlugin.BOM_COORDINATES))
    implementation(platform("com.fasterxml.jackson:jackson-bom:2.22.4"))
    implementation("dev.ilmek:starter")
    implementation("dev.ilmek:ilmek-spring-web-adapter")
    implementation("org.springframework.boot:$webStarter")
    implementation("org.springframework.boot:spring-boot-starter-validation")
    implementation("org.springframework.boot:spring-boot-starter-data-jpa")
    runtimeOnly("com.h2database:h2")
    testImplementation("org.springframework.boot:spring-boot-starter-test")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<JavaCompile>().configureEach {
    options.release.set(17)
    options.encoding = "UTF-8"
    options.compilerArgs.add("-parameters")
}

tasks.test {
    useJUnitPlatform()
}

tasks.bootJar {
    archiveFileName.set("rest-notes-backend.jar")
}
