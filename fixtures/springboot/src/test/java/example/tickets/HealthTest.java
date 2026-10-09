package example.tickets;

import static org.assertj.core.api.Assertions.assertThat;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.web.client.TestRestTemplate;

@SpringBootTest(webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT)
class HealthTest {
    @Autowired TestRestTemplate client;

    @Test void healthIsAvailable() {
        assertThat(client.getForObject("/api/health", String.class)).contains("ok");
    }
}
