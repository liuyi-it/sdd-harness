package example.tickets;

import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import org.springframework.stereotype.Service;

@Service
// 工单仅存在于当前 JVM 的内存中，重启不会恢复旧工单。
public class TicketService {
    private final ConcurrentMap<String, Ticket> tickets = new ConcurrentHashMap<>();

    public Ticket create(long customerId, String title) {
        Ticket ticket;
        do {
            ticket = new Ticket(UUID.randomUUID().toString(), customerId, title, "OPEN");
        } while (tickets.putIfAbsent(ticket.ticketId(), ticket) != null);
        return ticket;
    }

    public Ticket find(String ticketId) {
        Ticket ticket = tickets.get(ticketId);
        if (ticket == null) {
            throw new TicketNotFoundException();
        }
        return ticket;
    }

    public record Ticket(String ticketId, long customerId, String title, String status) {}

    public static class TicketNotFoundException extends RuntimeException {}
}
