plugins {
    `java-library`
}

val bootVersion = providers.gradleProperty("bootVersion").getOrElse("4.1.2")
val bootLine = if (bootVersion.startsWith("3.")) "boot3" else "boot4"

group = "dev.ilmek"
version = "0.1.0-SNAPSHOT"
base.archivesName.set("ilmek-spring-web-adapter")

tasks.withType<JavaCompile>().configureEach {
    options.release.set(17)
    options.encoding = "UTF-8"
}

repositories {
    mavenCentral()
}

dependencies {
    api(project(":starter"))
    implementation(platform("com.fasterxml.jackson:jackson-bom:2.22.4"))
    implementation("com.fasterxml.jackson.core:jackson-databind")
    implementation("org.springframework:spring-webmvc")
    implementation("org.springframework:spring-test")
    compileOnly("jakarta.servlet:jakarta.servlet-api")
    testImplementation("org.junit.jupiter:junit-jupiter")
    testImplementation("org.springframework.boot:spring-boot-starter-test")
    testImplementation("org.springframework.boot:spring-boot-starter-data-jpa")
    testImplementation("org.springframework.boot:spring-boot-starter-validation")
    if (bootLine == "boot3") {
        testImplementation("org.springframework.boot:spring-boot-starter-web")
    } else {
        testImplementation("org.springframework.boot:spring-boot-starter-webmvc")
    }
    testRuntimeOnly("com.h2database:h2")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.compileTestJava {
    options.compilerArgs.add("-parameters")
}

tasks.test {
    useJUnitPlatform()
}
