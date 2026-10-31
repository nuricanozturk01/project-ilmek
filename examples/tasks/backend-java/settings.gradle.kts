pluginManagement {
    val bootVersion = providers.gradleProperty("bootVersion").getOrElse("4.1.2")
    plugins {
        id("org.springframework.boot") version bootVersion
    }
}

rootProject.name = "backend-java"

includeBuild("../../../java/ilmek-spring-boot-starter")
