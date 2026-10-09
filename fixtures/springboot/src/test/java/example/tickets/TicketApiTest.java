package example.tickets;

import static org.assertj.core.api.Assertions.assertThat;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.stream.IntStream;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.Arguments;
import org.junit.jupiter.params.provider.MethodSource;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.web.client.TestRestTemplate;
import org.springframework.core.ParameterizedTypeReference;
import org.springframework.http.HttpEntity;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpMethod;
import org.springframework.http.MediaType;
import org.springframework.http.ResponseEntity;

@SpringBootTest(webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT)
class TicketApiTest {
    private static final ParameterizedTypeReference<Map<String, Object>> JSON_MAP =
            new ParameterizedTypeReference<>() {};

    @Autowired TestRestTemplate client;

    @Test
    void createsAndReadsTrimmedTicket() {
        ResponseEntity<Map<String, Object>> created = post("{\"customer_id\":7,\"title\":\"  处理退货  \"}");

        assertThat(created.getStatusCode().value()).isEqualTo(201);
        Map<String, Object> body = created.getBody();
        assertThat(body).containsOnlyKeys("ticket_id", "customer_id", "title", "status");
        assertThat(body).containsEntry("customer_id", 7).containsEntry("title", "处理退货");
        assertThat(body).containsEntry("status", "OPEN");

        ResponseEntity<Map<String, Object>> read = client.exchange(
                "/api/tickets/" + body.get("ticket_id"), HttpMethod.GET, HttpEntity.EMPTY, JSON_MAP);

        assertThat(read.getStatusCode().value()).isEqualTo(200);
        assertThat(read.getBody()).isEqualTo(body);
    }

    @Test
    void returnsStableNotFoundError() {
        assertNotFound("unknown-ticket");
        assertNotFound("00000000-0000-0000-0000-000000000000");
    }

    @ParameterizedTest(name = "{0}")
    @MethodSource("invalidRequests")
    void rejectsInvalidRequests(String name, String json) {
        assertBadRequest(json);
    }

    static Stream<Arguments> invalidRequests() {
        return Stream.of(
                Arguments.of("missing customer_id", "{\"title\":\"x\"}"),
                Arguments.of("null customer_id", "{\"customer_id\":null,\"title\":\"x\"}"),
                Arguments.of("zero customer_id", "{\"customer_id\":0,\"title\":\"x\"}"),
                Arguments.of("negative customer_id", "{\"customer_id\":-1,\"title\":\"x\"}"),
                Arguments.of("decimal customer_id", "{\"customer_id\":1.5,\"title\":\"x\"}"),
                Arguments.of("string customer_id", "{\"customer_id\":\"1\",\"title\":\"x\"}"),
                Arguments.of("boolean customer_id", "{\"customer_id\":true,\"title\":\"x\"}"),
                Arguments.of(
                        "overflow customer_id",
                        "{\"customer_id\":9223372036854775808,\"title\":\"x\"}"),
                Arguments.of("missing title", "{\"customer_id\":1}"),
                Arguments.of("null title", "{\"customer_id\":1,\"title\":null}"),
                Arguments.of("numeric title", "{\"customer_id\":1,\"title\":1}"),
                Arguments.of("boolean title", "{\"customer_id\":1,\"title\":true}"),
                Arguments.of("array title", "{\"customer_id\":1,\"title\":[]}"),
                Arguments.of("blank title", "{\"customer_id\":1,\"title\":\" \\t\\n\"}"),
                Arguments.of(
                        "too long title",
                        "{\"customer_id\":1,\"title\":\"" + "中".repeat(121) + "\"}"),
                Arguments.of("unknown field", "{\"customer_id\":1,\"title\":\"x\",\"extra\":true}"),
                Arguments.of("camel case field", "{\"customerId\":1,\"title\":\"x\"}"),
                Arguments.of("malformed json", "{\"customer_id\":1,\"title\":\"x\""));
    }

    @Test
    void acceptsTitlesByUnicodeCodePointAndRejects121() {
        String chinese = "中".repeat(120);
        String emoji = "😀".repeat(120);

        assertThat(post("{\"customer_id\":1,\"title\":\"" + chinese + "\"}").getStatusCode().value())
                .isEqualTo(201);
        assertThat(post("{\"customer_id\":2,\"title\":\"" + emoji + "\"}").getStatusCode().value())
                .isEqualTo(201);
        assertBadRequest("{\"customer_id\":3,\"title\":\"" + "😀".repeat(121) + "\"}");
    }

    @Test
    void concurrentlyCreatesAndReadsDistinctTickets() throws Exception {
        ExecutorService executor = Executors.newFixedThreadPool(20);
        try {
            List<Future<ResponseEntity<Map<String, Object>>>> futures = IntStream.range(0, 20)
                    .mapToObj(index -> executor.submit(() -> post(
                            "{\"customer_id\":" + (index + 1) + ",\"title\":\"并发-" + index + "\"}")))
                    .toList();
            List<Map<String, Object>> created = new ArrayList<>();
            for (Future<ResponseEntity<Map<String, Object>>> future : futures) {
                ResponseEntity<Map<String, Object>> response = get(future);
                assertThat(response.getStatusCode().value()).isEqualTo(201);
                created.add(response.getBody());
            }

            Set<String> ids = new HashSet<>();
            for (Map<String, Object> ticket : created) {
                String id = String.valueOf(ticket.get("ticket_id"));
                ids.add(id);
                ResponseEntity<Map<String, Object>> read = client.exchange(
                        "/api/tickets/" + id, HttpMethod.GET, HttpEntity.EMPTY, JSON_MAP);
                assertThat(read.getStatusCode().value()).isEqualTo(200);
                assertThat(read.getBody()).isEqualTo(ticket);
            }
            assertThat(ids).hasSize(20);
        } finally {
            executor.shutdownNow();
            executor.awaitTermination(5, TimeUnit.SECONDS);
        }
    }

    private ResponseEntity<Map<String, Object>> post(String json) {
        HttpHeaders headers = new HttpHeaders();
        headers.setContentType(MediaType.APPLICATION_JSON);
        return client.exchange(
                "/api/tickets", HttpMethod.POST, new HttpEntity<>(json, headers), JSON_MAP);
    }

    private void assertBadRequest(String json) {
        ResponseEntity<Map<String, Object>> response = post(json);
        assertThat(response.getStatusCode().value()).isEqualTo(400);
        assertThat(response.getBody()).containsEntry("code", "BAD_REQUEST");
    }

    private void assertNotFound(String ticketId) {
        ResponseEntity<Map<String, Object>> response = client.exchange(
                "/api/tickets/" + ticketId, HttpMethod.GET, HttpEntity.EMPTY, JSON_MAP);
        assertThat(response.getStatusCode().value()).isEqualTo(404);
        assertThat(response.getBody()).containsEntry("code", "NOT_FOUND");
    }

    private ResponseEntity<Map<String, Object>> get(Future<ResponseEntity<Map<String, Object>>> future)
            throws InterruptedException, ExecutionException, TimeoutException {
        return future.get(10, TimeUnit.SECONDS);
    }
}
