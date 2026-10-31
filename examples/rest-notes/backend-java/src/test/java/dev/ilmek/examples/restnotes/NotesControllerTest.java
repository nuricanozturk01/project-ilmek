package dev.ilmek.examples.restnotes;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.hasSize;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.delete;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.header;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import static org.assertj.core.api.Assertions.assertThat;

import dev.ilmek.sdk.api.IlmekMethodRegistry;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.core.env.Environment;
import org.springframework.http.MediaType;
import org.springframework.test.context.TestPropertySource;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import org.springframework.web.context.WebApplicationContext;

@SpringBootTest
@TestPropertySource(properties = "spring.datasource.url=jdbc:h2:mem:notes-test;DB_CLOSE_DELAY=-1")
class NotesControllerTest {

    @Autowired
    private WebApplicationContext context;

    @Autowired
    private NoteRepository repository;

    @Autowired
    private IlmekMethodRegistry registry;

    @Autowired
    private Environment environment;

    private MockMvc mvc;

    @BeforeEach
    void setUp() {
        repository.deleteAll();
        mvc = MockMvcBuilders.webAppContextSetup(context).build();
    }

    private String create(String json) throws Exception {
        return mvc.perform(post("/api/notes").contentType(MediaType.APPLICATION_JSON).content(json))
                .andExpect(status().isCreated())
                .andReturn()
                .getResponse()
                .getHeader("Location");
    }

    @Test
    void adapterRegistersHttpMethodsAndDisablesTheListener() {
        assertThat(registry.find("http.read")).isPresent();
        assertThat(registry.find("http.write")).isPresent();
        assertThat(environment.getProperty("server.port")).isEqualTo("-1");
    }

    @Test
    void emptyListIsAnEmptyArray() throws Exception {
        mvc.perform(get("/api/notes")).andExpect(status().isOk()).andExpect(jsonPath("$", hasSize(0)));
    }

    @Test
    void createReturns201WithLocationAndBody() throws Exception {
        mvc.perform(post("/api/notes").contentType(MediaType.APPLICATION_JSON)
                        .content("{\"title\":\"milk\",\"body\":\"2 litres\"}"))
                .andExpect(status().isCreated())
                .andExpect(header().string("Location", containsString("/api/notes/")))
                .andExpect(jsonPath("$.title").value("milk"))
                .andExpect(jsonPath("$.body").value("2 litres"))
                .andExpect(jsonPath("$.id").isNumber());
    }

    @Test
    void getReturnsTheCreatedNoteAnd404WhenMissing() throws Exception {
        String location = create("{\"title\":\"a\",\"body\":\"b\"}");
        mvc.perform(get(location)).andExpect(status().isOk()).andExpect(jsonPath("$.title").value("a"));
        mvc.perform(get("/api/notes/999999")).andExpect(status().isNotFound()).andExpect(jsonPath("$.error").exists());
    }

    @Test
    void blankTitleIs400WithJsonErrorBody() throws Exception {
        mvc.perform(post("/api/notes").contentType(MediaType.APPLICATION_JSON).content("{\"title\":\"  \",\"body\":\"x\"}"))
                .andExpect(status().isBadRequest())
                .andExpect(jsonPath("$.error").value("validation failed"))
                .andExpect(jsonPath("$.fields.title").exists());
    }

    @Test
    void titleOver100CharactersIs400() throws Exception {
        String longTitle = "x".repeat(101);
        mvc.perform(post("/api/notes").contentType(MediaType.APPLICATION_JSON)
                        .content("{\"title\":\"" + longTitle + "\",\"body\":\"\"}"))
                .andExpect(status().isBadRequest())
                .andExpect(jsonPath("$.fields.title").exists());
    }

    @Test
    void titleOf100CharactersIsAccepted() throws Exception {
        create("{\"title\":\"" + "x".repeat(100) + "\",\"body\":\"\"}");
    }

    @Test
    void putReplacesAndMissingIs404() throws Exception {
        String location = create("{\"title\":\"old\",\"body\":\"old\"}");
        mvc.perform(put(location).contentType(MediaType.APPLICATION_JSON).content("{\"title\":\"new\",\"body\":\"newer\"}"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.title").value("new"));
        mvc.perform(get(location)).andExpect(jsonPath("$.body").value("newer"));
        mvc.perform(put("/api/notes/999999").contentType(MediaType.APPLICATION_JSON).content("{\"title\":\"t\"}"))
                .andExpect(status().isNotFound());
    }

    @Test
    void putWithInvalidTitleIs400() throws Exception {
        String location = create("{\"title\":\"ok\",\"body\":\"\"}");
        mvc.perform(put(location).contentType(MediaType.APPLICATION_JSON).content("{\"title\":\"\"}"))
                .andExpect(status().isBadRequest());
    }

    @Test
    void deleteReturns204ThenGetIs404AndSecondDeleteIs404() throws Exception {
        String location = create("{\"title\":\"gone\",\"body\":\"\"}");
        mvc.perform(delete(location)).andExpect(status().isNoContent());
        mvc.perform(get(location)).andExpect(status().isNotFound());
        mvc.perform(delete(location)).andExpect(status().isNotFound());
    }

    @Test
    void queryFiltersByTitleOrBodyIgnoringCase() throws Exception {
        create("{\"title\":\"Groceries\",\"body\":\"milk\"}");
        create("{\"title\":\"Work\",\"body\":\"report\"}");
        mvc.perform(get("/api/notes").param("q", "GROC")).andExpect(jsonPath("$", hasSize(1)));
        mvc.perform(get("/api/notes").param("q", "report")).andExpect(jsonPath("$", hasSize(1)));
        mvc.perform(get("/api/notes").param("q", "zzz")).andExpect(jsonPath("$", hasSize(0)));
        mvc.perform(get("/api/notes")).andExpect(jsonPath("$", hasSize(2)));
    }
}
