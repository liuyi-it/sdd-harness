package example.tickets;

import com.fasterxml.jackson.databind.JsonNode;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Set;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.http.converter.HttpMessageNotReadableException;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/tickets")
public class TicketController {
    private static final Set<String> REQUIRED_FIELDS = Set.of("customer_id", "title");

    private final TicketService ticketService;

    public TicketController(TicketService ticketService) {
        this.ticketService = ticketService;
    }

    @PostMapping
    public ResponseEntity<TicketService.Ticket> create(@RequestBody JsonNode request) {
        validateObject(request);
        validateFields(request);
        long customerId = validateCustomerId(request.get("customer_id"));
        String title = validateTitle(request.get("title"));
        return ResponseEntity.status(HttpStatus.CREATED).body(ticketService.create(customerId, title));
    }

    @GetMapping("/{ticketId}")
    public TicketService.Ticket find(@PathVariable String ticketId) {
        return ticketService.find(ticketId);
    }

    @ExceptionHandler(BadRequestException.class)
    public ResponseEntity<ErrorResponse> badRequest() {
        return ResponseEntity.badRequest().body(new ErrorResponse("BAD_REQUEST", "请求参数无效"));
    }

    @ExceptionHandler(HttpMessageNotReadableException.class)
    public ResponseEntity<ErrorResponse> malformedJson() {
        return ResponseEntity.badRequest().body(new ErrorResponse("BAD_REQUEST", "请求 JSON 无效"));
    }

    @ExceptionHandler(TicketService.TicketNotFoundException.class)
    public ResponseEntity<ErrorResponse> notFound() {
        return ResponseEntity.status(HttpStatus.NOT_FOUND)
                .body(new ErrorResponse("NOT_FOUND", "工单不存在"));
    }

    private void validateObject(JsonNode request) {
        if (request == null || !request.isObject()) {
            throw new BadRequestException();
        }
    }

    private void validateFields(JsonNode request) {
        Set<String> fields = new HashSet<>();
        Iterator<String> names = request.fieldNames();
        names.forEachRemaining(fields::add);
        if (!fields.equals(REQUIRED_FIELDS)) {
            throw new BadRequestException();
        }
    }

    private long validateCustomerId(JsonNode customerId) {
        if (customerId == null
                || !customerId.isIntegralNumber()
                || !customerId.canConvertToLong()
                || customerId.longValue() <= 0) {
            throw new BadRequestException();
        }
        return customerId.longValue();
    }

    private String validateTitle(JsonNode title) {
        if (title == null || !title.isTextual()) {
            throw new BadRequestException();
        }
        String stripped = title.textValue().strip();
        if (stripped.isEmpty() || stripped.codePointCount(0, stripped.length()) > 120) {
            throw new BadRequestException();
        }
        return stripped;
    }

    private record ErrorResponse(String code, String message) {}

    private static class BadRequestException extends RuntimeException {}
}
