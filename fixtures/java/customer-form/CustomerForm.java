import java.awt.BorderLayout;
import java.awt.GridLayout;
import java.nio.charset.StandardCharsets;
import javax.swing.JButton;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

public final class CustomerForm {
    private CustomerForm() {}

    public static void main(String[] args) {
        SwingUtilities.invokeLater(CustomerForm::show);
    }

    private static void show() {
        JFrame frame = new JFrame("Greentic Java Fixture");
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);

        JTextField customerName = new JTextField(24);
        customerName.setName("customer_name");
        customerName.getAccessibleContext().setAccessibleName("customer_name");
        customerName.getAccessibleContext().setAccessibleDescription("Customer name input");

        JTextField email = new JTextField(24);
        email.setName("email");
        email.getAccessibleContext().setAccessibleName("email");
        email.getAccessibleContext().setAccessibleDescription("Email input");

        JLabel confirmationId = new JLabel("");
        confirmationId.setName("confirmation_id");
        confirmationId.getAccessibleContext().setAccessibleName("confirmation_id");
        confirmationId.getAccessibleContext().setAccessibleDescription("Confirmation id output");

        JButton save = new JButton("Save");
        save.setName("Save");
        save.getAccessibleContext().setAccessibleName("Save");
        save.getAccessibleContext().setAccessibleDescription("Save customer form");
        save.addActionListener(event -> confirmationId.setText(
            confirmationId(customerName.getText(), email.getText())
        ));

        JPanel form = new JPanel(new GridLayout(0, 2, 8, 8));
        form.add(new JLabel("Customer name"));
        form.add(customerName);
        form.add(new JLabel("Email"));
        form.add(email);
        form.add(save);
        form.add(confirmationId);

        frame.getContentPane().add(form, BorderLayout.CENTER);
        frame.pack();
        frame.setLocationByPlatform(true);
        frame.setVisible(true);
    }

    private static String confirmationId(String customerName, String email) {
        String input = "customer_name=" + customerName + "\nemail=" + email;
        long hash = 0xcbf29ce484222325L;
        for (byte value : input.getBytes(StandardCharsets.UTF_8)) {
            hash ^= value & 0xffL;
            hash *= 0x100000001b3L;
        }
        return String.format("CONF-%08x", hash & 0xffffffffL);
    }
}
