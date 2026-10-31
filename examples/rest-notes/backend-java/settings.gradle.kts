pluginManagement {
    val bootVersion = providers.gradleProperty("bootVersion").getOrElse("4.1.2")
    plugins {
        id("org.springframework.boot") version bootVersion
    }
}

rootProject.name = "rest-notes-backend"

includeBuild("../../../java/ilmek-spring-boot-starter") {
    dependencySubstitution {
        substitute(module("dev.ilmek:ilmek-spring-web-adapter")).using(project(":web-adapter"))
    }
}
